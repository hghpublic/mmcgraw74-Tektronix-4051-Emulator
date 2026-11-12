function errorText(error) {
  return error instanceof Error ? error.message : String(error);
}

function editorFilenameTitle(filename) {
  const parts = filename.split(/[\\/]/);
  const basename = parts[parts.length - 1];
  const stem = basename.replace(/\.bas$/i, "");
  const sizeSeparator = stem.lastIndexOf(" ");
  if (sizeSeparator < 20) {
    return stem;
  }

  const number = Number(stem.slice(0, 7).trim());
  if (
    !Number.isInteger(number)
    || number < 1
    || number > 254
    || stem.slice(7, 15) !== "ASCII   "
    || stem.slice(15, 20) !== "PROG "
    || !/^\d+$/.test(stem.slice(sizeSeparator + 1))
  ) {
    return stem;
  }

  return stem.slice(20, sizeSeparator).trim() || stem;
}

export function bindBasicEditorUi({ emulator, wasm, onError, onRun }) {
  const elements = {
    filename: document.querySelector("#basic-editor-filename"),
    load: document.querySelector("#basic-editor-load"),
    loadPicker: document.querySelector("#basic-editor-load-picker"),
    save: document.querySelector("#basic-editor-save"),
    runMode: document.querySelector("#basic-editor-run-mode"),
    run: document.querySelector("#basic-editor-run"),
    source: document.querySelector("#basic-editor-source"),
    lineNumbers: document.querySelector("#basic-editor-line-numbers"),
    status: document.querySelector("#basic-editor-status"),
  };
  for (const [name, element] of Object.entries(elements)) {
    if (!element) {
      throw new Error(`BASIC editor UI is missing its ${name} element`);
    }
  }

  const listeners = new AbortController();
  let operationBusy = false;
  let latestSnapshot = null;
  let latestWorkflowBusy = false;
  let useTemporaryFile = false;

  function updateRunMode() {
    const mode = useTemporaryFile ? "Temporary File" : "Keyboard";
    elements.runMode.textContent = `Run mode: ${mode}`;
    elements.runMode.setAttribute("aria-pressed", String(useTemporaryFile));
  }

  function setStatus(message, failed = false) {
    elements.status.textContent = message;
    elements.status.classList.toggle("error", failed);
  }

  function renderLineNumbers() {
    const lineCount = Math.max(1, elements.source.value.split(/\r\n|\r|\n/).length);
    elements.lineNumbers.textContent = Array.from(
      { length: lineCount },
      (_, index) => String(index + 1),
    ).join("\n");
    elements.lineNumbers.scrollTop = elements.source.scrollTop;
  }

  function setBusy(busy) {
    operationBusy = busy;
    update(latestSnapshot, latestWorkflowBusy);
  }

  function update(snapshot, workflowBusy = false) {
    latestSnapshot = snapshot;
    latestWorkflowBusy = workflowBusy;
    const inputSequenceBusy = snapshot?.auto_load_state === wasm.BrowserAutoLoadState.Typing;
    const unavailable = !snapshot
      || snapshot.file_io_busy
      || inputSequenceBusy
      || operationBusy
      || workflowBusy;
    for (const control of [
      elements.filename,
      elements.load,
      elements.loadPicker,
      elements.save,
      elements.runMode,
      elements.source,
    ]) {
      control.disabled = unavailable;
    }
    elements.run.disabled = unavailable
      || !elements.source.value.trim()
      || Boolean(snapshot?.runtime_error);
  }

  function reportFailure(label, error) {
    const message = `${label}: ${errorText(error)}`;
    setStatus(message, true);
    onError(new Error(message));
  }

  async function loadFile(file) {
    setBusy(true);
    try {
      if (!file.name.toLowerCase().endsWith(".bas")) {
        throw new Error("Select a BASIC source file with a .bas extension");
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      const source = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      elements.source.value = source.replace(/^\uFEFF/, "");
      elements.filename.value = editorFilenameTitle(file.name);
      renderLineNumbers();
      setStatus(`Loaded ${file.name}.`);
    } catch (error) {
      reportFailure("BASIC file load failed", error);
    } finally {
      setBusy(false);
    }
  }

  function onLoad() {
    elements.loadPicker.click();
  }

  function onLoadChange() {
    const [file] = elements.loadPicker.files ?? [];
    elements.loadPicker.value = "";
    if (!file) {
      setStatus("Load cancelled; the editor is unchanged.");
      return;
    }
    void loadFile(file);
  }

  async function saveFile() {
    setBusy(true);
    try {
      const prepared = emulator.prepare_editor_file(
        elements.filename.value,
        elements.source.value,
      );
      const name = prepared.name;
      const bytes = prepared.bytes();
      if (typeof window.showDirectoryPicker !== "function") {
        throw new Error("Selecting an output folder is not supported by this browser");
      }

      let directoryHandle;
      try {
        directoryHandle = await window.showDirectoryPicker({ mode: "readwrite" });
      } catch (error) {
        if (error?.name === "AbortError") {
          setStatus("Save cancelled; no file was written.");
          return;
        }
        throw error;
      }

      let fileHandle;
      try {
        fileHandle = await directoryHandle.getFileHandle(name);
      } catch (error) {
        if (error?.name !== "NotFoundError") {
          throw error;
        }
      }

      if (fileHandle && !window.confirm(`Replace existing ${name}?`)) {
        setStatus("Save cancelled; the existing file was not changed.");
        return;
      }
      if (!fileHandle) {
        fileHandle = await directoryHandle.getFileHandle(name, { create: true });
      }

      const writable = await fileHandle.createWritable();
      try {
        await writable.write(bytes);
        await writable.close();
      } catch (writeError) {
        try {
          await writable.abort();
        } catch (abortError) {
          throw new Error(
            `Save failed: ${errorText(writeError)}; closing the partial file failed: ${errorText(abortError)}`,
          );
        }
        throw writeError;
      }
      setStatus(`Saved ${name} in the selected folder.`);
    } catch (error) {
      reportFailure("BASIC file save failed", error);
    } finally {
      setBusy(false);
    }
  }

  async function runSource() {
    if (typeof onRun !== "function") {
      reportFailure("BASIC program run failed", new Error("The emulator run handler is unavailable"));
      return;
    }
    setBusy(true);
    setStatus("Preparing BASIC source...");
    try {
      await onRun(elements.source.value, useTemporaryFile, setStatus);
    } catch (error) {
      reportFailure("BASIC program run failed", error);
    } finally {
      setBusy(false);
    }
  }

  elements.load.addEventListener("click", onLoad, { signal: listeners.signal });
  elements.loadPicker.addEventListener("change", onLoadChange, { signal: listeners.signal });
  elements.save.addEventListener("click", () => void saveFile(), { signal: listeners.signal });
  elements.runMode.addEventListener("click", () => {
    useTemporaryFile = !useTemporaryFile;
    updateRunMode();
    setStatus(`Run mode changed to ${useTemporaryFile ? "Temporary File" : "Keyboard"}.`);
  }, { signal: listeners.signal });
  elements.run.addEventListener("click", () => void runSource(), { signal: listeners.signal });
  elements.source.addEventListener("input", renderLineNumbers, { signal: listeners.signal });
  elements.source.addEventListener("scroll", renderLineNumbers, { signal: listeners.signal });
  updateRunMode();
  renderLineNumbers();

  return {
    update,
    dispose() {
      listeners.abort();
    },
  };
}
