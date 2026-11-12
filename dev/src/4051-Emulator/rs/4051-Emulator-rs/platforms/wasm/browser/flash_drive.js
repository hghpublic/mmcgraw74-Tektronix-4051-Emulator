function errorText(error) {
  return error instanceof Error ? error.message : String(error);
}

function workerRequest() {
  const worker = new Worker(new URL("./flash_drive_worker.js", import.meta.url), {
    type: "module",
  });
  let resolve;
  let reject;
  let settled = false;
  const result = new Promise((resolveResult, rejectResult) => {
    resolve = resolveResult;
    reject = rejectResult;
  });
  result.catch(() => {});
  const settle = (callback, value) => {
    if (settled) {
      return;
    }
    settled = true;
    callback(value);
  };
  worker.addEventListener("message", (event) => {
    if (event.data?.ok) {
      settle(resolve, event.data);
    } else {
      settle(reject, new Error(event.data?.error ?? "Flash Drive worker failed"));
    }
  });
  worker.addEventListener("error", (event) => {
    event.preventDefault();
    settle(reject, new Error(event.message || "Flash Drive worker failed"));
  });
  worker.addEventListener("messageerror", () => {
    settle(reject, new Error("Flash Drive worker returned an unreadable result"));
  });
  return { worker, result };
}

function transferableView(view) {
  if (view.byteOffset === 0 && view.byteLength === view.buffer.byteLength) {
    return view;
  }
  return view.slice();
}

function uniqueBuffers(views) {
  return [...new Set(views.map((view) => view.buffer))];
}

const archiveChunkBytes = 1024 * 1024;

function isZipFile(name) {
  return name.toLowerCase().endsWith(".zip");
}

function descriptionText(units) {
  let text = "";
  let invalid = false;
  for (let index = 0; index < units.length; index += 1) {
    const unit = units[index];
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = units[index + 1];
      if (next === undefined || next < 0xdc00 || next > 0xdfff) {
        invalid = true;
      } else {
        text += String.fromCharCode(unit, next);
        index += 1;
        continue;
      }
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      invalid = true;
    }
    text += String.fromCharCode(unit);
  }
  if (invalid) {
    const raw = Array.from(units, (unit) => unit.toString(16).padStart(4, "0")).join(" ");
    return { text: "Invalid UTF-16 description", title: `Raw UTF-16 units: ${raw}`, invalid };
  }
  return { text: text.replace(/ +$/u, ""), title: "", invalid };
}

function directoryEntries(snapshot) {
  const entries = [];
  for (let index = 0; index < snapshot.directory_len; index += 1) {
    const entry = snapshot.directory_entry(index);
    if (!entry) {
      throw new Error(`Flash Drive directory row ${index} is unavailable`);
    }
    const description = descriptionText(entry.description_utf16());
    entries.push({
      number: entry.number,
      fileType: entry.file_type,
      usage: entry.usage,
      byteLength: entry.byte_len,
      description,
    });
  }
  return entries;
}

function addCell(row, text, className = "", title = "") {
  const cell = document.createElement("td");
  cell.textContent = text;
  if (className) {
    cell.className = className;
  }
  if (title) {
    cell.title = title;
  }
  row.append(cell);
}

function appendNotice(list, notice) {
  const item = document.createElement("li");
  item.textContent = `${notice.label}: ${notice.message}`;
  if (notice.error) {
    item.className = "error";
  }
  list.append(item);
}

export function bindFlashDriveUi({ emulator, wasm, onError }) {
  if ((typeof emulator !== "object" && typeof emulator !== "function") || emulator === null) {
    throw new TypeError("emulator is required");
  }
  if (!wasm?.BrowserImportSelection || typeof wasm.max_flash_archive_file_bytes !== "function") {
    throw new TypeError("WASM Flash Drive bindings are required");
  }
  if (typeof onError !== "function") {
    throw new TypeError("onError must be a function");
  }

  const elements = {
    importFiles: document.querySelector("#import-files"),
    importFilesPicker: document.querySelector("#import-files-picker"),
    restoreArchive: document.querySelector("#restore-archive"),
    restoreArchivePicker: document.querySelector("#restore-archive-picker"),
    clearFlashDrive: document.querySelector("#clear-flash-drive"),
    autoLoad: document.querySelector("#auto-load"),
    cancelAutoLoad: document.querySelector("#cancel-auto-load"),
    exportNumber: document.querySelector("#export-file-number"),
    exportFile: document.querySelector("#export-file"),
    exportArchive: document.querySelector("#export-archive"),
    launchFile: document.querySelector("#launch-file"),
    fileCount: document.querySelector("#flash-file-count"),
    directory: document.querySelector("#flash-directory"),
    directoryBody: document.querySelector("#flash-directory-body"),
    status: document.querySelector("#flash-status"),
    notices: document.querySelector("#flash-notices"),
  };
  for (const [name, element] of Object.entries(elements)) {
    if (!element) {
      throw new Error(`Flash Drive UI is missing its ${name} element`);
    }
  }

  const window = document.defaultView;
  if (!window) {
    throw new Error("Flash Drive UI requires a browser window");
  }

  const listeners = new AbortController();
  const objectUrls = new Set();
  const operationControls = [
    elements.importFiles,
    elements.restoreArchive,
    elements.clearFlashDrive,
    elements.autoLoad,
    elements.cancelAutoLoad,
    elements.exportNumber,
    elements.exportFile,
    elements.exportArchive,
    elements.launchFile,
    elements.importFilesPicker,
    elements.restoreArchivePicker,
  ];
  const maxArchiveBytes = wasm.max_flash_archive_file_bytes();
  let latestSnapshot = null;
  let selectedNumber = null;
  let directorySignature = null;
  let noticeSignature = null;
  let operationBusy = false;
  let directory = [];

  function setStatus(message, error = false) {
    elements.status.textContent = message;
    elements.status.classList.toggle("error", error);
  }

  function setBusy(busy) {
    operationBusy = busy;
    if (latestSnapshot) {
      update(latestSnapshot);
    } else {
      for (const control of operationControls) {
        control.disabled = busy;
      }
    }
  }

  function updateSelection() {
    for (const row of elements.directoryBody.querySelectorAll("tr[data-file-number]")) {
      row.setAttribute(
        "aria-selected",
        String(Number(row.dataset.fileNumber) === selectedNumber),
      );
    }
    elements.launchFile.disabled = operationBusy
      || Boolean(latestSnapshot?.file_io_busy)
      || latestSnapshot?.auto_load_state === wasm.BrowserAutoLoadState.Typing
      || !latestSnapshot?.running
      || !latestSnapshot?.basic_prompt_ready
      || Boolean(latestSnapshot?.runtime_error)
      || selectedNumber === null;
  }

  function renderDirectory(snapshot) {
    const entries = directoryEntries(snapshot);
    const signature = JSON.stringify(entries.map((entry) => [
      entry.number,
      entry.fileType,
      entry.usage,
      String(entry.byteLength),
      entry.description.text,
      entry.description.title,
    ]));
    if (signature === directorySignature) {
      return;
    }
    directorySignature = signature;
    directory = entries;
    if (!directory.some((entry) => entry.number === selectedNumber)) {
      selectedNumber = null;
    }
    elements.fileCount.textContent =
      `${directory.length} stored file${directory.length === 1 ? "" : "s"}`;
    const rows = directory.map((entry) => {
      const row = document.createElement("tr");
      row.dataset.fileNumber = String(entry.number);
      row.id = `flash-file-${entry.number}`;
      addCell(row, String(entry.number));
      addCell(row, entry.fileType ?? "Missing type", entry.fileType ? "" : "missing-metadata");
      const missingUsage = (entry.usage === null || entry.usage === undefined)
        && entry.fileType !== "NEW"
        && entry.fileType !== "LAST";
      addCell(
        row,
        entry.usage ?? (missingUsage ? "Missing usage" : "-"),
        missingUsage ? "missing-metadata" : "",
      );
      addCell(row, String(entry.byteLength));
      addCell(
        row,
        entry.description.text,
        entry.description.invalid ? "missing-metadata" : "",
        entry.description.title,
      );
      return row;
    });
    elements.directoryBody.replaceChildren(...rows);
    updateSelection();
  }

  function renderNotices(snapshot) {
    const notices = [];
    for (let index = 0; index < snapshot.import_notice_count; index += 1) {
      const notice = snapshot.import_notice(index);
      if (!notice) {
        throw new Error(`Flash Drive import notice ${index} is unavailable`);
      }
      notices.push({ label: "Import", error: notice.error, message: notice.message });
    }
    const exportNotice = snapshot.export_notice;
    if (exportNotice) {
      notices.push({
        label: "Export",
        error: exportNotice.error,
        message: exportNotice.message,
      });
    }
    const launchNotice = snapshot.program_launch_notice;
    if (launchNotice) {
      notices.push({
        label: "Program launch",
        error: launchNotice.error,
        message: launchNotice.message,
      });
    }
    if (snapshot.auto_load_error) {
      notices.push({ label: "Auto Load", error: true, message: snapshot.auto_load_error });
    }
    const autoLoadState = snapshot.auto_load_state;
    if (autoLoadState === wasm.BrowserAutoLoadState.Typing) {
      notices.push({ label: "Auto Load", error: false, message: "Typing file-1 commands" });
    } else if (autoLoadState === wasm.BrowserAutoLoadState.Sent) {
      notices.push({
        label: "Auto Load",
        error: false,
        message: "Commands sent; program execution may still fail",
      });
    } else if (autoLoadState === wasm.BrowserAutoLoadState.Cancelled) {
      notices.push({
        label: "Auto Load",
        error: true,
        message: "Cancelled; a partial command may remain in the BASIC editor",
      });
    }
    const signature = JSON.stringify(notices);
    if (signature === noticeSignature) {
      return;
    }
    noticeSignature = signature;
    elements.notices.replaceChildren();
    for (const notice of notices) {
      appendNotice(elements.notices, notice);
    }
  }

  function update(snapshot) {
    latestSnapshot = snapshot;
    renderDirectory(snapshot);
    renderNotices(snapshot);
    const blocked = operationBusy || snapshot.file_io_busy;
    const typingAutoLoad = snapshot.auto_load_state === wasm.BrowserAutoLoadState.Typing;
    elements.importFiles.disabled = blocked || typingAutoLoad;
    elements.restoreArchive.disabled = blocked || typingAutoLoad;
    elements.clearFlashDrive.disabled = blocked || typingAutoLoad || directory.length === 0;
    elements.cancelAutoLoad.hidden = !typingAutoLoad;
    elements.cancelAutoLoad.disabled = blocked || !typingAutoLoad;
    elements.importFilesPicker.disabled = blocked || typingAutoLoad;
    elements.restoreArchivePicker.disabled = blocked || typingAutoLoad;
    elements.exportNumber.disabled = blocked || typingAutoLoad;
    elements.exportFile.disabled = blocked || typingAutoLoad;
    elements.exportArchive.disabled = blocked || typingAutoLoad;
    elements.autoLoad.disabled =
      blocked
      || typingAutoLoad
      || !snapshot.running
      || !snapshot.basic_prompt_ready
      || Boolean(snapshot.runtime_error);
    updateSelection();
  }

  function revokeObjectUrl(url) {
    if (!objectUrls.delete(url)) {
      return;
    }
    URL.revokeObjectURL(url);
  }

  function downloadBytes(bytes, name, type) {
    const blob = new Blob([bytes], { type });
    const url = URL.createObjectURL(blob);
    objectUrls.add(url);
    const link = document.createElement("a");
    link.href = url;
    link.download = name;
    link.hidden = true;
    document.body.append(link);
    try {
      link.click();
    } finally {
      link.remove();
      window.requestAnimationFrame(() => revokeObjectUrl(url));
    }
  }

  async function importFiles(files) {
    setBusy(true);
    setStatus(
      `Processing ${files.length} selected file(s); controls are disabled until import completes.`,
    );
    let request = null;
    try {
      const items = [];
      for (const file of files) {
        if (isZipFile(file.name) && file.size > maxArchiveBytes) {
          items.push({
            kind: "error",
            message: `Import failed for ${file.name}: ZIP exceeds the ${maxArchiveBytes / (1024 * 1024)} MiB compressed file limit`,
          });
          continue;
        }
        items.push({ kind: "file", file });
      }

      let results = items.filter((item) => item.kind === "error");
      if (items.some((item) => item.kind === "file")) {
        request = workerRequest();
        request.worker.postMessage({ type: "prepare-import", items });
        results = (await request.result).results;
      }

      const selection = new wasm.BrowserImportSelection();
      for (const result of results) {
        if (result.kind === "error") {
          selection.add_prepared_error(result.message);
        } else {
          selection.add_prepared_file(result.name, result.bytes);
        }
      }
      if (selection.is_empty()) {
        setStatus("No selected files were read; the directory was not changed.", true);
        return;
      }
      const selectedCount = selection.len();
      emulator.import_selected_files(selection);
      setStatus(`Processed ${selectedCount} selected file(s). See the ordered notices below.`);
      update(emulator.advance());
    } catch (error) {
      setStatus(`Flash Drive import failed: ${errorText(error)}`, true);
      onError(error);
    } finally {
      request?.worker.terminate();
      setBusy(false);
    }
  }

  async function restoreArchive(file) {
    if (file.size > maxArchiveBytes) {
      setStatus(
        `Archive restore failed: ZIP exceeds the ${maxArchiveBytes / (1024 * 1024)} MiB compressed file limit`,
        true,
      );
      return;
    }
    if (!window.confirm("Replace the entire in-memory Flash Drive from this archive?")) {
      setStatus("Archive restore cancelled; the current directory is unchanged.");
      return;
    }

    setBusy(true);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const count = emulator.replace_flash_drive_archive(true, bytes);
      if (count === undefined || count === null) {
        throw new Error("The confirmed archive replacement did not return a file count");
      }
      setStatus(`Flash Drive archive restored: ${count} file(s).`);
      update(emulator.advance());
    } catch (error) {
      setStatus(`Flash Drive archive restore failed: ${errorText(error)}`, true);
      onError(error);
    } finally {
      setBusy(false);
    }
  }

  function clearFlashDrive() {
    if (!window.confirm("Clear all files from the in-memory Flash Drive?")) {
      setStatus("Flash Drive clear cancelled; the directory is unchanged.");
      return;
    }

    const fileCount = directory.length;
    setBusy(true);
    try {
      emulator.clear_flash_drive();
      setStatus(`Flash Drive cleared: ${fileCount} file${fileCount === 1 ? "" : "s"} removed.`);
      update(emulator.advance());
    } catch (error) {
      setStatus(`Flash Drive clear failed: ${errorText(error)}`, true);
      onError(error);
      update(emulator.advance());
    } finally {
      setBusy(false);
    }
  }

  function onImportFiles() {
    elements.importFilesPicker.click();
  }

  function onImportFilesChange() {
    const files = Array.from(elements.importFilesPicker.files ?? []);
    elements.importFilesPicker.value = "";
    if (files.length === 0) {
      setStatus("File import cancelled; the directory is unchanged.");
      return;
    }
    void importFiles(files);
  }

  function onRestoreArchive() {
    elements.restoreArchivePicker.click();
  }

  function onRestoreArchiveChange() {
    const [file] = Array.from(elements.restoreArchivePicker.files ?? []);
    elements.restoreArchivePicker.value = "";
    if (!file) {
      setStatus("Archive selection cancelled; the directory is unchanged.");
      return;
    }
    void restoreArchive(file);
  }

  function onAutoLoad() {
    try {
      emulator.auto_load();
      setStatus("Auto Load queued through the shared input sequencer.");
      update(emulator.advance());
    } catch (error) {
      setStatus(`Auto Load failed: ${errorText(error)}`, true);
      onError(error);
      update(emulator.advance());
    }
  }

  function onCancelAutoLoad() {
    try {
      emulator.release_input();
      setStatus("Auto Load cancelled; a partial command may remain in the BASIC editor.");
      update(emulator.advance());
    } catch (error) {
      setStatus(`Auto Load cancellation failed: ${errorText(error)}`, true);
      onError(error);
      update(emulator.advance());
    }
  }

  function launchFile(number) {
    try {
      emulator.launch_file(number);
      setStatus(`Program launch requested for Flash Drive file ${number}.`);
    } catch (error) {
      setStatus(`Program launch failed: ${errorText(error)}`, true);
      onError(error);
    }
    update(emulator.advance());
  }

  function onLaunchFile() {
    if (selectedNumber !== null) {
      launchFile(selectedNumber);
    }
  }

  function onExportFile() {
    try {
      const number = elements.exportNumber.valueAsNumber;
      if (!Number.isInteger(number) || number < 1 || number > 254) {
        throw new RangeError("file number must be between 1 and 254");
      }
      const file = emulator.export_file(number);
      downloadBytes(file.bytes(), file.name, "application/octet-stream");
      setStatus(`Downloaded Flash Drive file ${number} as ${file.name}.`);
      update(emulator.advance());
    } catch (error) {
      setStatus(`Flash Drive file download failed: ${errorText(error)}`, true);
      onError(error);
      update(emulator.advance());
    }
  }

  async function onExportArchive() {
    setBusy(true);
    let request = null;
    let snapshotActive = false;
    try {
      request = workerRequest();
      const fileCount = emulator.begin_archive_export_snapshot();
      snapshotActive = true;
      update(emulator.advance());
      request.worker.postMessage({ type: "start-archive" });
      for (let index = 0; index < fileCount; index += 1) {
        const file = emulator.archive_export_snapshot_file(index);
        if (!file) {
          throw new Error(`Flash Drive archive snapshot file ${index} is unavailable`);
        }
        const description = transferableView(file.description_utf16());
        request.worker.postMessage(
          {
            type: "archive-file-start",
            number: file.number,
            fileTypeCode: file.file_type_code,
            usageCode: file.usage_code,
            description,
            byteLength: file.byte_len,
          },
          [description.buffer],
        );
        for (let offset = 0; offset < file.byte_len; offset += archiveChunkBytes) {
          const length = Math.min(archiveChunkBytes, file.byte_len - offset);
          const bytes = transferableView(file.bytes_range(offset, length));
          request.worker.postMessage(
            { type: "archive-file-chunk", bytes },
            uniqueBuffers([bytes]),
          );
          if (offset + length < file.byte_len || index + 1 < fileCount) {
            await new Promise((resolve) => window.requestAnimationFrame(resolve));
          }
        }
      }
      emulator.finish_archive_export_snapshot();
      snapshotActive = false;
      request.worker.postMessage({ type: "finish-archive" });
      const result = await request.result;
      downloadBytes(result.bytes, "tek4051-flash-drive.zip", "application/zip");
      emulator.publish_archive_export_notice(false, "Flash Drive archive exported");
      setStatus("Downloaded the Flash Drive archive.");
      update(emulator.advance());
    } catch (error) {
      if (snapshotActive) {
        emulator.finish_archive_export_snapshot();
        snapshotActive = false;
      }
      const message = `Flash Drive archive export failed: ${errorText(error)}`;
      emulator.publish_archive_export_notice(true, message);
      setStatus(`Flash Drive archive download failed: ${errorText(error)}`, true);
      onError(error);
      update(emulator.advance());
    } finally {
      if (snapshotActive) {
        emulator.finish_archive_export_snapshot();
      }
      request?.worker.terminate();
      setBusy(false);
    }
  }

  function onDirectoryClick(event) {
    const target = event.target instanceof Element ? event.target : event.target.parentElement;
    const row = target?.closest("tr[data-file-number]");
    if (!row || !elements.directoryBody.contains(row)) {
      return;
    }
    selectedNumber = Number(row.dataset.fileNumber);
    elements.directory.focus({ preventScroll: true });
    updateSelection();
  }

  function onDirectoryDoubleClick(event) {
    const target = event.target instanceof Element ? event.target : event.target.parentElement;
    const row = target?.closest("tr[data-file-number]");
    if (row && elements.directoryBody.contains(row)) {
      selectedNumber = Number(row.dataset.fileNumber);
      updateSelection();
      launchFile(selectedNumber);
    }
  }

  function onDirectoryKeyDown(event) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (directory.length === 0) {
        return;
      }
      const current = directory.findIndex((entry) => entry.number === selectedNumber);
      const next = current < 0
        ? (event.key === "ArrowDown" ? 0 : directory.length - 1)
        : event.key === "ArrowDown"
          ? Math.min(current + 1, directory.length - 1)
          : Math.max(current - 1, 0);
      selectedNumber = directory[next].number;
      updateSelection();
      document.querySelector(`#flash-file-${selectedNumber}`)?.scrollIntoView({
        block: "nearest",
      });
      event.preventDefault();
    } else if (event.key === "Enter" && selectedNumber !== null) {
      launchFile(selectedNumber);
      event.preventDefault();
    }
  }

  function onPageHide() {
    for (const url of objectUrls) {
      URL.revokeObjectURL(url);
    }
    objectUrls.clear();
  }

  const signal = { signal: listeners.signal };
  elements.importFiles.addEventListener("click", onImportFiles, signal);
  elements.importFilesPicker.addEventListener("change", onImportFilesChange, signal);
  elements.importFilesPicker.addEventListener(
    "cancel",
    () => setStatus("File import cancelled; the directory is unchanged."),
    signal,
  );
  elements.restoreArchive.addEventListener("click", onRestoreArchive, signal);
  elements.clearFlashDrive.addEventListener("click", clearFlashDrive, signal);
  elements.restoreArchivePicker.addEventListener("change", onRestoreArchiveChange, signal);
  elements.restoreArchivePicker.addEventListener(
    "cancel",
    () => setStatus("Archive selection cancelled; the directory is unchanged."),
    signal,
  );
  elements.autoLoad.addEventListener("click", onAutoLoad, signal);
  elements.cancelAutoLoad.addEventListener("click", onCancelAutoLoad, signal);
  elements.exportFile.addEventListener("click", onExportFile, signal);
  elements.exportArchive.addEventListener("click", onExportArchive, signal);
  elements.launchFile.addEventListener("click", onLaunchFile, signal);
  elements.directory.addEventListener("click", onDirectoryClick, signal);
  elements.directory.addEventListener("dblclick", onDirectoryDoubleClick, signal);
  elements.directory.addEventListener("keydown", onDirectoryKeyDown, signal);
  window.addEventListener("pagehide", onPageHide, signal);

  return {
    update,
    destroy() {
      listeners.abort();
      onPageHide();
    },
  };
}
