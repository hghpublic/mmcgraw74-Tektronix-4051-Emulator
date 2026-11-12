import { attachBrowserKeyboard, bindPanelKeyButton } from "./input.js";
import { bindBasicInputStatusUi } from "./basic_input_status.js";
import { bindBasicEditorUi } from "./basic_editor.js";
import { bindDisplayCamera } from "./display_camera.js";
import { bindFlashDriveUi } from "./flash_drive.js";
import { renderWorkloadGraph, retainWorkloadSample } from "./workload_graph.js";

const WORKLOAD_BUDGET_MS = 10;
const BASIC_TEST_FIRST_LINE = 100;
const BASIC_DISPLAY_WIDTH = 130;
const BASIC_DISPLAY_HEIGHT = 100;
const BASIC_DISPLAY_RASTER_WIDTH = 1024;
const BASIC_DISPLAY_RASTER_HEIGHT = 780;
const BASIC_DISPLAY_GRID_DIVISIONS = 9;
const BASIC_DISPLAY_MAX_X = BASIC_DISPLAY_WIDTH;
const BASIC_DISPLAY_MAX_Y = BASIC_DISPLAY_HEIGHT;
const BASIC_DISPLAY_X_OFFSET = 16;
const BASIC_DISPLAY_Y_OFFSET = 10;
const BASIC_DISPLAY_CIRCLE_RADIUS = 45;
const BASIC_DISPLAY_CIRCLE_CENTER_X = BASIC_DISPLAY_WIDTH / 2;
const BASIC_DISPLAY_CIRCLE_CENTER_Y = BASIC_DISPLAY_HEIGHT / 2;
const BASIC_DISPLAY_CIRCLE_POINTS = Array.from({ length: 24 }, (_, index) => {
  const angle = 2 * Math.PI * index / 24;
  return [
    Math.round(BASIC_DISPLAY_CIRCLE_CENTER_X + BASIC_DISPLAY_CIRCLE_RADIUS * Math.cos(angle)),
    Math.round(BASIC_DISPLAY_CIRCLE_CENTER_Y + BASIC_DISPLAY_CIRCLE_RADIUS * Math.sin(angle)),
  ];
});
const BASIC_DISPLAY_CIRCLE_CARDINAL_POINTS = [0, 6, 12, 18]
  .map((index) => BASIC_DISPLAY_CIRCLE_POINTS[index]);

function buildGridTestLines() {
  return [
    "PAGE",
    `S=${BASIC_DISPLAY_MAX_X}/${BASIC_DISPLAY_GRID_DIVISIONS}`,
    `T=${BASIC_DISPLAY_MAX_Y}/${BASIC_DISPLAY_GRID_DIVISIONS}`,
    `FOR I=1 TO ${BASIC_DISPLAY_GRID_DIVISIONS - 1}`,
    "MOVE S*I,0",
    `DRAW S*I,${BASIC_DISPLAY_MAX_Y}`,
    "MOVE 0,T*I",
    `DRAW ${BASIC_DISPLAY_MAX_X},T*I`,
    "NEXT I",
    "MOVE 0,0",
    `DRAW ${BASIC_DISPLAY_MAX_X},0`,
    `DRAW ${BASIC_DISPLAY_MAX_X},${BASIC_DISPLAY_MAX_Y}`,
    `DRAW 0,${BASIC_DISPLAY_MAX_Y}`,
    "DRAW 0,0",
    "END",
  ];
}

function buildSingleCircleTestLines() {
  const [firstPoint, ...remainingPoints] = BASIC_DISPLAY_CIRCLE_POINTS;
  // Precomputed vertices keep the test independent of BASIC trig support.
  return [
    "PAGE",
    `MOVE ${firstPoint[0]},${firstPoint[1]}`,
    ...remainingPoints.map(([x, y]) => `DRAW ${x},${y}`),
    `DRAW ${firstPoint[0]},${firstPoint[1]}`,
    "END",
  ];
}

const BASIC_DISPLAY_TESTS = [
  {
    id: "grid",
    label: "Grid",
    lines: buildGridTestLines(),
    minimumLitPixels: 5_000,
  },
  {
    id: "snake",
    label: "Snake fill",
    lines: [
      "PAGE",
      "MOVE 0,0",
      `DRAW ${BASIC_DISPLAY_MAX_X},0`,
      `DRAW ${BASIC_DISPLAY_MAX_X},20`,
      "DRAW 0,20",
      "DRAW 0,40",
      `DRAW ${BASIC_DISPLAY_MAX_X},40`,
      `DRAW ${BASIC_DISPLAY_MAX_X},60`,
      "DRAW 0,60",
      "DRAW 0,80",
      `DRAW ${BASIC_DISPLAY_MAX_X},80`,
      `DRAW ${BASIC_DISPLAY_MAX_X},${BASIC_DISPLAY_MAX_Y}`,
      "END",
    ],
    minimumLitPixels: 5_000,
  },
  {
    id: "single-circle",
    label: "Single circle",
    lines: buildSingleCircleTestLines(),
    minimumLitPixels: 1_500,
    expectedPoints: BASIC_DISPLAY_CIRCLE_CARDINAL_POINTS,
  },
];
const elements = {
  canvas: document.querySelector("#screen"),
  webgpuCanvas: document.querySelector("#screen-webgpu"),
  rendererStatus: document.querySelector("#renderer-status"),
  displaySurface: document.querySelector("#display-surface"),
  keyboardInput: document.querySelector("#keyboard-input"),
  flashDriveTab: document.querySelector("#flash-drive-tab"),
  editorTab: document.querySelector("#editor-tab"),
  controlsTab: document.querySelector("#controls-tab"),
  workloadTab: document.querySelector("#workload-tab"),
  testTab: document.querySelector("#test-tab"),
  basicTestButtons: [...document.querySelectorAll("[data-basic-test]")],
  basicTestStatus: document.querySelector("#basic-test-status"),
  vectorZoomToggle: document.querySelector("#vector-zoom-enabled"),
  vectorZoomStatus: document.querySelector("#vector-zoom-status"),
  vectorZoomModeControl: document.querySelector("#vector-zoom-mode-control"),
  vectorZoomMode: document.querySelector("#vector-zoom-mode"),
  workloadSummary: document.querySelector("#workload-summary"),
  workloadGraph: document.querySelector("#workload-graph"),
  runPause: document.querySelector("#run-pause"),
  reset: document.querySelector("#reset"),
  focusKeyboard: document.querySelector("#focus-keyboard"),
  capsLock: document.querySelector("#caps-lock"),
  speed: document.querySelector("#speed"),
  granularity: document.querySelector("#granularity"),
  busyStatus: document.querySelector("#status-busy"),
  breakStatus: document.querySelector("#status-break"),
  ioStatus: document.querySelector("#status-io"),
  powerStatus: document.querySelector("#status-power"),
  errorMessage: document.querySelector("#error-message"),
};
const toolTabs = [
  elements.flashDriveTab,
  elements.editorTab,
  elements.controlsTab,
  elements.workloadTab,
  elements.testTab,
];

let emulator = null;
let latestSnapshot = null;
let frameError = null;
let basicTestActive = false;
let editorRunActive = false;
let vectorZoomEnabled = false;
let vectorZoomMode = "raster";
let lastRevision = null;
let lastPresentedWebGpuRevision = null;
let canvasContext = null;
let flashDriveBinding = null;
let basicEditorBinding = null;
let basicInputStatusBinding = null;
let displayCamera = null;
const workloadStartedAt = performance.now();
const workloadSamples = [];

function invalidateWebGpuPresentation() {
  lastPresentedWebGpuRevision = null;
}

window.addEventListener("resize", invalidateWebGpuPresentation);
window.addEventListener("pageshow", invalidateWebGpuPresentation);
document.addEventListener("visibilitychange", invalidateWebGpuPresentation);

function errorText(error) {
  return error instanceof Error ? error.message : String(error);
}

function showError(error) {
  elements.errorMessage.textContent = errorText(error);
  elements.errorMessage.hidden = false;
}

function clearError() {
  elements.errorMessage.textContent = "";
  elements.errorMessage.hidden = true;
}

function updateVectorZoomModeUi() {
  const hidden = !vectorZoomEnabled;
  if (elements.vectorZoomModeControl.hidden !== hidden) {
    elements.vectorZoomModeControl.hidden = hidden;
  }
  if (elements.vectorZoomMode.value !== vectorZoomMode) {
    elements.vectorZoomMode.value = vectorZoomMode;
  }
  const disabled = !vectorZoomEnabled
    || Boolean(latestSnapshot?.file_io_busy)
    || basicTestActive
    || editorRunActive;
  if (elements.vectorZoomMode.disabled !== disabled) {
    elements.vectorZoomMode.disabled = disabled;
  }
}

function updateVectorZoomCamera(transform) {
  if (!vectorZoomEnabled) {
    return;
  }
  try {
    emulator.set_vector_zoom_camera(transform.zoom, transform.panX, transform.panY);
  } catch (error) {
    showError(new Error(`Vector camera update failed: ${errorText(error)}`));
  }
}

function runCommand(label, operation) {
  try {
    operation();
    clearError();
  } catch (error) {
    showError(new Error(`${label}: ${errorText(error)}`));
  }
}

function renderPixels(snapshot) {
  elements.canvas.hidden = snapshot.webgpu_active;
  elements.webgpuCanvas.hidden = !snapshot.webgpu_active;
  if (snapshot.webgpu_active) {
    return;
  }

  if (snapshot.revision === lastRevision) {
    return;
  }

  const width = snapshot.width;
  const height = snapshot.height;
  elements.canvas.width = width;
  elements.canvas.height = height;
  elements.canvas.style.aspectRatio = `${width} / ${height}`;
  canvasContext ??= elements.canvas.getContext("2d", { alpha: false });
  if (!canvasContext) {
    throw new Error("The browser could not create the emulator display canvas");
  }
  const imageData = canvasContext.createImageData(width, height);
  const pixels = snapshot.pixels();
  for (let source = 0, target = 0; source < pixels.length; source += 1, target += 4) {
    imageData.data[target + 1] = pixels[source];
    imageData.data[target + 3] = 0xff;
  }
  canvasContext.putImageData(imageData, 0, 0);
  lastRevision = snapshot.revision;
}

function updateRendererStatus(snapshot) {
  const renderer = snapshot.extended_webgpu_active
    ? "Extended WebGPU"
    : snapshot.webgpu_active
      ? "WebGPU"
      : "CPU reference";
  const fallback = snapshot.render_error;
  elements.rendererStatus.dataset.renderer = snapshot.webgpu_active
    ? snapshot.extended_webgpu_active
      ? "extended-webgpu"
      : "webgpu"
    : fallback
      ? "cpu-fallback"
      : "cpu";
  elements.rendererStatus.textContent = fallback && !snapshot.webgpu_active
    ? "CPU fallback"
    : renderer;
  elements.rendererStatus.title = fallback ?? "";
  elements.rendererStatus.setAttribute(
    "aria-label",
    fallback && !snapshot.webgpu_active
      ? `CPU reference renderer. ${fallback}`
      : `${renderer} renderer`,
  );
}

function updateStatusLamps(snapshot) {
  const lamps = [
    [elements.busyStatus, snapshot.busy, "BUSY"],
    [elements.breakStatus, snapshot.break_active, "BREAK"],
    [elements.ioStatus, snapshot.io, "I/O"],
    [elements.powerStatus, snapshot.running, "POWER"],
  ];
  for (const [lamp, active, label] of lamps) {
    const ariaLabel = `${label} ${active ? "on" : "off"}`;
    if (lamp.classList.contains("is-active") !== active) {
      lamp.classList.toggle("is-active", active);
    }
    if (lamp.getAttribute("aria-label") !== ariaLabel) {
      lamp.setAttribute("aria-label", ariaLabel);
    }
  }
}

function activateToolTab(selectedTab, focus = false) {
  for (const tab of toolTabs) {
    const active = tab === selectedTab;
    tab.setAttribute("aria-selected", String(active));
    tab.tabIndex = active ? 0 : -1;
    document.getElementById(tab.getAttribute("aria-controls")).hidden = !active;
  }
  if (focus) {
    selectedTab.focus();
  }
}

function bindToolTabs() {
  const tabs = toolTabs;
  for (const [index, tab] of tabs.entries()) {
    tab.addEventListener("click", () => activateToolTab(tab));
    tab.addEventListener("keydown", (event) => {
      let nextIndex;
      if (event.key === "ArrowRight") {
        nextIndex = (index + 1) % tabs.length;
      } else if (event.key === "ArrowLeft") {
        nextIndex = (index + tabs.length - 1) % tabs.length;
      } else if (event.key === "Home") {
        nextIndex = 0;
      } else if (event.key === "End") {
        nextIndex = tabs.length - 1;
      } else {
        return;
      }
      event.preventDefault();
      activateToolTab(tabs[nextIndex], true);
    });
  }
}

function waitForAnimationFrame() {
  return new Promise((resolve) => window.requestAnimationFrame(resolve));
}

function assertBasicTestCanContinue() {
  if (frameError) {
    throw new Error(`The emulator frame loop failed: ${errorText(frameError)}`);
  }
  if (latestSnapshot?.runtime_error) {
    throw new Error(latestSnapshot.runtime_error);
  }
  if (latestSnapshot && !latestSnapshot.running) {
    throw new Error("The emulator stopped before the BASIC test completed");
  }
}

async function waitForBasicPrompt() {
  let sawRunning = false;
  for (;;) {
    if (frameError) {
      throw new Error(`The emulator frame loop failed: ${errorText(frameError)}`);
    }
    if (latestSnapshot?.runtime_error) {
      throw new Error(latestSnapshot.runtime_error);
    }
    if (latestSnapshot?.running) {
      sawRunning = true;
      if (latestSnapshot.basic_prompt_ready) {
        return;
      }
    } else if (sawRunning) {
      throw new Error("The emulator stopped before the BASIC prompt returned");
    }
    await waitForAnimationFrame();
  }
}

async function waitForBasicTestCompletion() {
  for (;;) {
    assertBasicTestCanContinue();
    if (
      emulator.pending_input_transitions() === 0
      && latestSnapshot?.basic_prompt_ready
    ) {
      return;
    }
    await waitForAnimationFrame();
  }
}

async function waitForEditorProgramSubmission(wasm) {
  let sawTyping = false;
  for (;;) {
    if (frameError) {
      throw new Error(`The emulator frame loop failed: ${errorText(frameError)}`);
    }
    if (latestSnapshot?.runtime_error) {
      throw new Error(latestSnapshot.runtime_error);
    }
    if (latestSnapshot && !latestSnapshot.running) {
      throw new Error("The emulator stopped before the editor program was submitted");
    }
    if (latestSnapshot?.auto_load_state === wasm.BrowserAutoLoadState.Typing) {
      sawTyping = true;
    }
    if (sawTyping && latestSnapshot?.auto_load_state === wasm.BrowserAutoLoadState.Sent) {
      return;
    }
    if (sawTyping && latestSnapshot?.auto_load_state === wasm.BrowserAutoLoadState.Cancelled) {
      throw new Error("Editor input was cancelled; the machine may contain partial source lines");
    }
    await waitForAnimationFrame();
  }
}

async function runEditorProgram(source, useTemporaryFile, wasm, reportStatus) {
  if (basicTestActive || editorRunActive) {
    throw new Error("Another BASIC input workflow is already active");
  }
  if (useTemporaryFile && typeof emulator.queue_editor_file_source !== "function") {
    throw new Error("Temporary File mode needs the current WASM bindings; rebuild and reload the emulator");
  }
  emulator.validate_editor_source(source);

  editorRunActive = true;
  clearError();
  updateControls(latestSnapshot, wasm);
  try {
    reportStatus("Resetting the machine for a clean BASIC program...");
    emulator.reset();
    latestSnapshot = emulator.snapshot();
    if (!latestSnapshot?.running) {
      emulator.set_running(true);
      latestSnapshot = emulator.snapshot();
    }
    reportStatus("Waiting for the machine-detected BASIC prompt...");
    await waitForBasicPrompt();
    if (useTemporaryFile) {
      reportStatus("Loading the editor source from private temporary tape storage...");
      emulator.queue_editor_file_source(source);
    } else {
      reportStatus("Submitting numbered source through the BASIC keyboard...");
      emulator.queue_editor_source(source);
    }
    await waitForEditorProgramSubmission(wasm);
    reportStatus(useTemporaryFile
      ? "Temporary BASIC file loaded; the program may still be running."
      : "Program commands were sent; the BASIC program may still be running.");
  } finally {
    editorRunActive = false;
    if (latestSnapshot) {
      updateControls(latestSnapshot, wasm);
    }
  }
}

function assertBasicTestOutput(test) {
  if (!latestSnapshot) {
    throw new Error(`${test.label} test has no display snapshot to validate`);
  }

  const pixels = latestSnapshot.pixels();
  let litPixels = 0;
  for (const intensity of pixels) {
    if (intensity > 0) {
      litPixels += 1;
    }
  }
  if (litPixels < test.minimumLitPixels) {
    throw new Error(
      `Expected at least ${test.minimumLitPixels} lit display pixels, observed ${litPixels}`,
    );
  }
  for (const [x, y] of test.expectedPoints ?? []) {
    const centerX = Math.round(
      BASIC_DISPLAY_X_OFFSET + x * BASIC_DISPLAY_RASTER_WIDTH / BASIC_DISPLAY_WIDTH,
    );
    const centerY = latestSnapshot.height - 1
      - Math.round(
        BASIC_DISPLAY_Y_OFFSET + y * BASIC_DISPLAY_RASTER_HEIGHT / BASIC_DISPLAY_HEIGHT,
      );
    let isLit = false;
    for (let yOffset = -1; yOffset <= 1 && !isLit; yOffset += 1) {
      for (let xOffset = -1; xOffset <= 1; xOffset += 1) {
        const pixelX = centerX + xOffset;
        const pixelY = centerY + yOffset;
        if (
          pixelX >= 0
          && pixelX < latestSnapshot.width
          && pixelY >= 0
          && pixelY < latestSnapshot.height
          && pixels[pixelY * latestSnapshot.width + pixelX] > 0
        ) {
          isLit = true;
          break;
        }
      }
    }
    if (!isLit) {
      throw new Error(`Expected display geometry near (${x}, ${y}) was not rendered`);
    }
  }
}

async function runBasicTest(test, wasm) {
  if (basicTestActive || editorRunActive) {
    return;
  }
  basicTestActive = true;
  clearError();
  elements.basicTestStatus.classList.remove("error");
  elements.basicTestStatus.textContent = `Preparing the ${test.label} BASIC test...`;
  updateControls(latestSnapshot, wasm);

  try {
    if (latestSnapshot?.runtime_error) {
      throw new Error(latestSnapshot.runtime_error);
    }
    if (!latestSnapshot?.running) {
      emulator.set_running(true);
    }
    elements.basicTestStatus.textContent = `Waiting for the BASIC prompt before the ${test.label} test...`;
    await waitForBasicPrompt();
    const lines = test.lines.map(
      (line, index) => `${BASIC_TEST_FIRST_LINE + index * 10} ${line}`,
    );
    elements.basicTestStatus.textContent = `Running the ${test.label} BASIC program...`;
    emulator.queue_basic_program(lines);
    await waitForBasicTestCompletion();
    assertBasicTestOutput(test);
    elements.basicTestStatus.textContent = `${test.label} test complete.`;
  } catch (error) {
    let message = `${test.label} test failed: ${errorText(error)}`;
    try {
      emulator.release_input();
    } catch (releaseError) {
      message += `; input release failed: ${errorText(releaseError)}`;
    }
    elements.basicTestStatus.classList.add("error");
    elements.basicTestStatus.textContent = message;
    showError(new Error(message));
  } finally {
    basicTestActive = false;
    if (latestSnapshot) {
      updateControls(latestSnapshot, wasm);
    }
  }
}

function bindBasicTests(wasm) {
  for (const test of BASIC_DISPLAY_TESTS) {
    const button = elements.basicTestButtons.find(
      (candidate) => candidate.dataset.basicTest === test.id,
    );
    if (!button) {
      throw new Error(`BASIC test UI is missing its ${test.id} button`);
    }
    button.addEventListener("click", () => {
      void runBasicTest(test, wasm);
    });
  }
}

function updateWorkload(snapshot, nowMilliseconds) {
  const average = workloadSamples.length === 0
    ? 0
    : workloadSamples.reduce((sum, sample) => sum + sample.workloadPercent, 0)
      / workloadSamples.length;
  const peak = workloadSamples.reduce(
    (maximum, sample) => Math.max(maximum, sample.workloadPercent),
    0,
  );
  const speedLabel = elements.speed.selectedOptions[0]?.textContent.trim();
  if (!speedLabel) {
    throw new Error(`Unknown CPU speed index ${snapshot.speed_index}`);
  }
  const historySeconds = workloadSamples.length > 1
    ? workloadSamples[workloadSamples.length - 1].elapsedSeconds
      - workloadSamples[0].elapsedSeconds
    : 0;
  elements.workloadSummary.dataset.averagePercent = String(average);
  elements.workloadSummary.dataset.peakPercent = String(peak);
  elements.workloadSummary.dataset.sampleCount = String(workloadSamples.length);
  elements.workloadSummary.dataset.historySeconds = String(historySeconds);
  const summary = `${speedLabel} | average ${average.toFixed(0)}% | peak ${peak.toFixed(0)}% | 100% = one ${WORKLOAD_BUDGET_MS} ms budget`;
  if (elements.workloadSummary.textContent !== summary) {
    elements.workloadSummary.textContent = summary;
  }
  renderWorkloadGraph(
    elements.workloadGraph,
    workloadSamples,
    (nowMilliseconds - workloadStartedAt) / 1000,
  );
}

function updateControls(snapshot, wasm) {
  latestSnapshot = snapshot;
  renderPixels(snapshot);
  updateRendererStatus(snapshot);
  updateStatusLamps(snapshot);
  if (!snapshot.extended_webgpu_active && vectorZoomEnabled) {
    try {
      emulator.set_vector_zoom_enabled(false);
      vectorZoomEnabled = false;
      vectorZoomMode = "raster";
      displayCamera?.setVectorPresentation(false);
      elements.vectorZoomStatus.textContent =
        "Vector scene capture stopped because Extended WebGPU is no longer active.";
    } catch (error) {
      showError(new Error(`Could not release vector scene capture: ${errorText(error)}`));
    }
  }
  elements.vectorZoomToggle.checked = vectorZoomEnabled;
  elements.vectorZoomToggle.disabled =
    snapshot.file_io_busy || basicTestActive || editorRunActive || !snapshot.extended_webgpu_active;
  updateVectorZoomModeUi();
  basicInputStatusBinding?.update(snapshot);
  flashDriveBinding?.update(snapshot);
  basicEditorBinding?.update(snapshot, basicTestActive || editorRunActive);
  const runPauseLabel = snapshot.running ? "Pause" : "Start";
  if (elements.runPause.textContent !== runPauseLabel) {
    elements.runPause.textContent = runPauseLabel;
  }
  const pressed = String(snapshot.running);
  if (elements.runPause.getAttribute("aria-pressed") !== pressed) {
    elements.runPause.setAttribute("aria-pressed", pressed);
  }
  elements.speed.value = String(snapshot.speed_index);
  updateWorkload(snapshot, performance.now());
  elements.granularity.value = snapshot.display_granularity === wasm.BrowserGranularity.Fine
    ? "Fine"
    : snapshot.display_granularity === wasm.BrowserGranularity.Coarse
      ? "Coarse"
      : "Mixed";
  elements.capsLock.checked = snapshot.caps_lock;
  for (const control of [
    elements.runPause,
    elements.reset,
    elements.speed,
    elements.granularity,
    elements.capsLock,
    ...document.querySelectorAll("[data-tek-code]"),
    ...elements.basicTestButtons,
  ]) {
    control.disabled = snapshot.file_io_busy || basicTestActive || editorRunActive;
  }

  if (snapshot.runtime_error) {
    showError(snapshot.runtime_error);
  } else if (snapshot.settings_error) {
    showError(snapshot.settings_error);
  }
}

function enableControls() {
  document.querySelectorAll("button:disabled, input:disabled, select:disabled").forEach((control) => {
    control.disabled = false;
  });
}

async function initialize() {
  try {
    const extendedWebGpu = new URLSearchParams(window.location.search).get("renderer") === "extended";
    const wasm = await import("/target/wasm/pkg/tek4051_emulator.js");
    await wasm.default();
    emulator = new wasm.BrowserEmulator();
    const initialSnapshot = emulator.snapshot();
    elements.webgpuCanvas.width = initialSnapshot.width;
    elements.webgpuCanvas.height = initialSnapshot.height;
    elements.webgpuCanvas.style.aspectRatio =
      `${initialSnapshot.width} / ${initialSnapshot.height}`;
    try {
      if (extendedWebGpu) {
        await emulator.initialize_extended_webgpu(elements.webgpuCanvas);
      } else {
        await emulator.initialize_webgpu(elements.webgpuCanvas);
      }
    } catch (error) {
      showError(new Error(
        `WebGPU initialization failed; CPU reference rendering remains active: ${errorText(error)}`,
      ));
    }
    basicInputStatusBinding = bindBasicInputStatusUi();
    flashDriveBinding = bindFlashDriveUi({ emulator, wasm, onError: showError });
    basicEditorBinding = bindBasicEditorUi({
      emulator,
      wasm,
      onError: showError,
      onRun: (source, useTemporaryFile, reportStatus) => (
        runEditorProgram(source, useTemporaryFile, wasm, reportStatus)
      ),
    });

    const inputBinding = attachBrowserKeyboard({
      surface: elements.displaySurface,
      keyboardInput: elements.keyboardInput,
      emulator,
      mapBrowserKey: wasm.map_browser_key_with_shift,
      onError: showError,
    });
    displayCamera = bindDisplayCamera(
      elements.displaySurface,
      (transform) => updateVectorZoomCamera(transform),
    );

    const panelBindings = [...document.querySelectorAll("[data-tek-code]")].map((button) => (
      bindPanelKeyButton({
        button,
        tekCode: Number(button.dataset.tekCode),
        shiftOverride: button.dataset.tekShift === undefined
          ? undefined
          : button.dataset.tekShift === "true",
        emulator,
        onError: showError,
      })
    ));

    elements.focusKeyboard.addEventListener("click", () => {
      elements.keyboardInput.focus({ preventScroll: true });
    });
    elements.runPause.addEventListener("click", () => {
      if (latestSnapshot) {
        runCommand("Run/Pause failed", () => {
          emulator.set_running(!latestSnapshot.running);
        });
      }
    });
    elements.reset.addEventListener("click", () => {
      runCommand("Reset failed", () => emulator.reset());
    });
    elements.speed.addEventListener("change", (event) => {
      runCommand("Speed change failed", () => emulator.set_speed(Number(event.target.value)));
    });
    elements.granularity.addEventListener("change", (event) => {
      runCommand("Display change failed", () => {
        emulator.set_display_granularity(wasm.BrowserGranularity[event.target.value]);
      });
    });
    elements.capsLock.addEventListener("change", (event) => {
      runCommand("Caps Lock change failed", () => emulator.set_caps_lock(event.target.checked));
    });
    elements.vectorZoomToggle.addEventListener("change", (event) => {
      const enabled = event.target.checked;
      try {
        emulator.set_vector_zoom_enabled(enabled);
        if (enabled) {
          const transform = displayCamera.getViewTransform();
          emulator.set_vector_zoom_mode(false);
          emulator.set_vector_zoom_camera(transform.zoom, transform.panX, transform.panY);
          vectorZoomMode = "raster";
        } else {
          vectorZoomMode = "raster";
        }
        vectorZoomEnabled = enabled;
        displayCamera.setVectorPresentation(enabled && vectorZoomMode === "vector");
        updateVectorZoomModeUi();
        elements.vectorZoomStatus.textContent = enabled
          ? "Vector scene capture is enabled in Raster mode; select Vector in the menu to use high-definition zoom."
          : "Vector scene capture is disabled.";
        clearError();
      } catch (error) {
        if (enabled) {
          try {
            emulator.set_vector_zoom_enabled(false);
            vectorZoomEnabled = false;
          } catch (cleanupError) {
            vectorZoomEnabled = true;
            showError(new Error(
              `Vector scene capture change failed: ${errorText(error)}; `
              + `capture cleanup failed: ${errorText(cleanupError)}`,
            ));
          }
          updateVectorZoomModeUi();
        }
        elements.vectorZoomToggle.checked = vectorZoomEnabled;
        if (!vectorZoomEnabled) {
          showError(new Error(`Vector scene capture change failed: ${errorText(error)}`));
        }
      }
    });
    elements.vectorZoomMode.addEventListener("change", (event) => {
      const enabled = event.target.value === "vector";
      try {
        emulator.set_vector_zoom_mode(enabled);
        vectorZoomMode = enabled ? "vector" : "raster";
        displayCamera.setVectorPresentation(enabled);
        updateVectorZoomModeUi();
        clearError();
      } catch (error) {
        elements.vectorZoomMode.value = vectorZoomMode;
        showError(new Error(`Vector presentation change failed: ${errorText(error)}`));
      }
    });

    bindToolTabs();
    bindBasicTests(wasm);
    enableControls();
    const initialFrame = emulator.advance();
    updateControls(initialFrame, wasm);
    presentWebGpuFrame(initialFrame);
    window.addEventListener("pagehide", () => {
      inputBinding.destroy();
      panelBindings.forEach((binding) => binding.destroy());
      flashDriveBinding.destroy();
    }, { once: true });

    function frame() {
      try {
        const workStartedAt = performance.now();
        const snapshot = emulator.advance();
        const workFinishedAt = performance.now();
        if (latestSnapshot && snapshot.speed_index !== latestSnapshot.speed_index) {
          workloadSamples.length = 0;
        }
        const sample = {
          elapsedSeconds: (workFinishedAt - workloadStartedAt) / 1000,
          workloadPercent: (workFinishedAt - workStartedAt) / WORKLOAD_BUDGET_MS * 100,
        };
        retainWorkloadSample(workloadSamples, sample);
        updateControls(snapshot, wasm);
        presentWebGpuFrame(snapshot);
      } catch (error) {
        frameError = error;
        showError(error);
        return;
      }
      window.requestAnimationFrame(frame);
    }
    window.requestAnimationFrame(frame);
  } catch (error) {
    showError(new Error(
      `Could not start the WebAssembly emulator. Build the WebAssembly bindings as described in docs/wasm.typ. ${errorText(error)}`,
    ));
  }
}

function presentWebGpuFrame(snapshot) {
  const metrics = window.__wasmWebGpuSurfaceMetrics;
  if (metrics) {
    const revision = snapshot.revision.toString();
    if (metrics.lastFrameRevision !== revision) {
      metrics.revisionChanges += 1;
      metrics.lastFrameRevision = revision;
    }
  }
  if (!snapshot.webgpu_active) {
    invalidateWebGpuPresentation();
    return;
  }
  if (snapshot.revision === lastPresentedWebGpuRevision) {
    if (metrics) {
      metrics.skippedPresentations += 1;
    }
    return;
  }
  const startedAt = performance.now();
  try {
    emulator.present_webgpu();
    lastPresentedWebGpuRevision = snapshot.revision;
    if (metrics) {
      metrics.presentedRevisions += 1;
    }
  } catch (error) {
    invalidateWebGpuPresentation();
    showError(new Error(
      `WebGPU presentation failed; CPU reference rendering is being used: ${errorText(error)}`,
    ));
  } finally {
    if (metrics) {
      const elapsed = performance.now() - startedAt;
      metrics.presentationCalls += 1;
      metrics.totalPresentationMilliseconds += elapsed;
      metrics.peakPresentationMilliseconds = Math.max(
        metrics.peakPresentationMilliseconds,
        elapsed,
      );
    }
  }
}

initialize();
