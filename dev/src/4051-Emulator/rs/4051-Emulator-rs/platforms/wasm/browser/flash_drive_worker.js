import init, {
  BrowserArchiveWriter,
  BrowserImportSelection,
  max_flash_archive_file_bytes,
} from "/target/wasm/pkg/tek4051_emulator.js";

const wasmReady = init();
let archiveWriter = null;
let archiveError = null;
let pendingArchiveFile = null;

function errorText(error) {
  return error instanceof Error ? error.message : String(error);
}

function isZipFile(name) {
  return name.toLowerCase().endsWith(".zip");
}

function transferableView(view) {
  if (view.byteOffset === 0 && view.byteLength === view.buffer.byteLength) {
    return view;
  }
  return view.slice();
}

function finishArchiveFile() {
  const file = pendingArchiveFile;
  if (!file) {
    throw new Error("archive worker received bytes without file metadata");
  }
  const bytes = new Uint8Array(file.receivedBytes);
  let offset = 0;
  for (const chunk of file.chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  archiveWriter.add_file(
    file.number,
    file.fileTypeCode,
    file.usageCode,
    file.description,
    bytes,
  );
  pendingArchiveFile = null;
}

async function prepareImport(items) {
  await wasmReady;
  const selection = new BrowserImportSelection();
  for (const item of items) {
    if (item.kind === "error") {
      selection.add_prepared_error(item.message);
      continue;
    }

    const file = item.file;
    if (isZipFile(file.name) && file.size > max_flash_archive_file_bytes()) {
      selection.add_error(
        file.name,
        `ZIP exceeds the ${max_flash_archive_file_bytes() / (1024 * 1024)} MiB compressed file limit`,
      );
      continue;
    }
    try {
      selection.add_file(file.name, new Uint8Array(await file.arrayBuffer()));
    } catch (error) {
      selection.add_error(file.name, errorText(error));
    }
  }

  selection.prepare_selected_files();
  const results = [];
  const transfers = new Set();
  for (let index = 0; index < selection.len(); index += 1) {
    if (selection.result_is_error(index)) {
      results.push({ kind: "error", message: selection.result_error(index) });
      continue;
    }
    const file = selection.result_file(index);
    if (!file) {
      throw new Error(`prepared Flash Drive import result ${index} is unavailable`);
    }
    const bytes = transferableView(file.bytes());
    transfers.add(bytes.buffer);
    results.push({ kind: "file", name: file.name, bytes });
  }
  self.postMessage({ ok: true, results }, [...transfers]);
}

async function handleMessage(message) {
  await wasmReady;
  switch (message.type) {
    case "prepare-import":
      await prepareImport(message.items);
      break;
    case "start-archive":
      archiveWriter = new BrowserArchiveWriter();
      archiveError = null;
      pendingArchiveFile = null;
      break;
    case "archive-file-start":
      if (!archiveWriter) {
        throw new Error("archive worker received a file before its start message");
      }
      if (pendingArchiveFile) {
        throw new Error("archive worker received new file metadata before the previous file ended");
      }
      pendingArchiveFile = {
        number: message.number,
        fileTypeCode: message.fileTypeCode,
        usageCode: message.usageCode,
        description: message.description,
        expectedBytes: message.byteLength,
        receivedBytes: 0,
        chunks: [],
      };
      if (pendingArchiveFile.expectedBytes === 0) {
        finishArchiveFile();
      }
      break;
    case "archive-file-chunk":
      if (!archiveWriter || !pendingArchiveFile) {
        throw new Error("archive worker received file bytes without active file metadata");
      }
      pendingArchiveFile.receivedBytes += message.bytes.byteLength;
      if (pendingArchiveFile.receivedBytes > pendingArchiveFile.expectedBytes) {
        throw new Error("archive worker received more file bytes than declared");
      }
      pendingArchiveFile.chunks.push(message.bytes);
      if (pendingArchiveFile.receivedBytes === pendingArchiveFile.expectedBytes) {
        finishArchiveFile();
      }
      break;
    case "finish-archive": {
      if (!archiveWriter) {
        throw new Error("archive worker received a finish message before its start message");
      }
      if (pendingArchiveFile) {
        throw new Error("archive worker received an incomplete file");
      }
      if (archiveError) {
        throw new Error(archiveError);
      }
      const bytes = transferableView(archiveWriter.finish());
      self.postMessage({ ok: true, bytes }, [bytes.buffer]);
      archiveWriter = null;
      break;
    }
    default:
      throw new Error(`unknown Flash Drive worker request: ${message.type}`);
  }
}

let queue = Promise.resolve();
self.addEventListener("message", (event) => {
  queue = queue
    .then(() => handleMessage(event.data))
    .catch((error) => {
      archiveError ??= errorText(error);
      self.postMessage({ ok: false, error: archiveError });
    });
});
