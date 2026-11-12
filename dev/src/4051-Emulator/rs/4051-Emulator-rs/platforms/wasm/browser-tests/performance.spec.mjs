import { expect, test } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import {
  installWebGpuSurfaceMetrics,
  readWebGpuSurfaceMetrics,
  resetWebGpuSurfaceMetrics,
} from "./webgpu-surface-metrics.mjs";
import {
  installWebGpuBufferMetrics,
  measureWebGpuQueueDrain,
  readWebGpuBufferMetrics,
  resetWebGpuBufferMetrics,
} from "./webgpu-buffer-metrics.mjs";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";
const workloadWindowSeconds = 2.9;
const speeds = [
  { label: "x1", index: "0" },
  { label: "x2", index: "1" },
  { label: "x5", index: "2" },
  { label: "x10", index: "3" },
  { label: "x20", index: "4" },
  { label: "x50", index: "5" },
  { label: "x100", index: "6" },
];
const vectorProgram = [
  "100 PAGE",
  "110 FOR I=1 TO 100",
  "120 MOVE I,I",
  "130 DRAW 1024-I,780-I",
  "140 NEXT I",
  "150 GO TO 110",
  "",
].join("\r");
const integerArithmeticProgram = [
  "100 PAGE",
  "110 A=0",
  "120 B=0",
  "130 C=0",
  "140 FOR I=1 TO 1000",
  "150 A=A+I",
  "160 B=B+I*2",
  "170 C=C+I*3",
  "180 NEXT I",
  "190 MOVE 510,390",
  "200 DRAW 512+(A-500500),390",
  "210 MOVE 510,400",
  "220 DRAW 512+(B-1001000),400",
  "230 MOVE 510,410",
  "240 DRAW 512+(C-1501500),410",
  "250 A=0",
  "260 B=0",
  "270 C=0",
  "280 FOR I=1 TO 1000",
  "290 A=A+I",
  "300 B=B+I*2",
  "310 C=C+I*3",
  "320 NEXT I",
  "330 A=0",
  "340 B=0",
  "350 C=0",
  "360 GOTO 280",
  "",
].join("\r");
const floatingPointArithmeticProgram = [
  "100 PAGE",
  "110 A=0.0",
  "120 B=0.0",
  "130 C=0.0",
  "140 FOR I=1 TO 1000",
  "150 A=A+0.125",
  "160 B=B+I/8",
  "170 C=C+I*0.25",
  "180 NEXT I",
  "190 MOVE 510,390",
  "200 DRAW 512+(A-125),390",
  "210 MOVE 510,400",
  "220 DRAW 512+(B-62562.5),400",
  "230 MOVE 510,410",
  "240 DRAW 512+(C-125125),410",
  "250 A=0.0",
  "260 B=0.0",
  "270 C=0.0",
  "280 FOR I=1 TO 1000",
  "290 A=A+0.125",
  "300 B=B+I/8",
  "310 C=C+I*0.25",
  "320 NEXT I",
  "330 A=0.0",
  "340 B=0.0",
  "350 C=0.0",
  "360 GOTO 280",
  "",
].join("\r");

async function launchFlashProgram(page, fileNumber) {
  await page.locator("#flash-drive-tab").click();
  await expect(page.locator("#flash-drive-panel")).toBeVisible();
  const row = page.locator(`#flash-directory-body tr[data-file-number="${fileNumber}"]`);
  await expect(row).toHaveCount(1);
  await row.click();
  await expect(page.locator("#launch-file")).toBeEnabled();
  await page.locator("#launch-file").click();
  await expect(page.locator("#flash-status")).toContainText(
    `Program launch requested for Flash Drive file ${fileNumber}`,
  );
}

async function waitForFramebufferPoints(page, points, workload) {
  await expect.poll(
    async () => page.locator("#screen").evaluate((surface, expectedPoints) => {
      const context = surface.getContext("2d");
      if (!context) {
        throw new Error("The reference display canvas has no 2D context");
      }
      const pixels = context.getImageData(0, 0, surface.width, surface.height).data;
      return expectedPoints.every(([x, y]) => {
        const pixelX = Math.round(x + 16);
        const pixelY = surface.height - 1 - Math.round(y + 10);
        return pixels[(pixelY * surface.width + pixelX) * 4 + 1] > 0;
      });
    }, points),
    {
      message: `${workload} arithmetic preflight should draw all expected result markers`,
      timeout: 30_000,
    },
  ).toBe(true);
}

test("reports WASM BASIC and vector-drawing workload by CPU speed", async ({ page }, testInfo) => {
  test.skip(
    process.env.RUN_WASM_PERF_BENCHMARKS !== "1",
    "Set RUN_WASM_PERF_BENCHMARKS=1 to run this local performance benchmark.",
  );
  test.setTimeout(180_000);
  await installWebGpuSurfaceMetrics(page);

  await page.addInitScript(() => {
    window.__wasmFrameIntervals = [];
    window.__wasmInputTimings = [];
    window.__wasmKeyDispatchTimings = [];
    window.__wasmKeySentAt = 0;
    document.addEventListener("keydown", (event) => {
      if (event.key.toLowerCase() === "a") {
        window.__wasmKeyDispatchTimings.push({
          name: "keydown",
          delayMs: performance.now() - window.__wasmKeySentAt,
        });
      }
    }, true);
    document.addEventListener("keyup", (event) => {
      if (event.key.toLowerCase() === "a") {
        window.__wasmKeyDispatchTimings.push({
          name: "keyup",
          delayMs: performance.now() - window.__wasmKeySentAt,
        });
      }
    }, true);
    if (PerformanceObserver.supportedEntryTypes?.includes("event")) {
      const inputTimingObserver = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (["keydown", "keyup", "beforeinput"].includes(entry.name)) {
            window.__wasmInputTimings.push({
              name: entry.name,
              inputDelayMs: entry.processingStart - entry.startTime,
              durationMs: entry.duration,
            });
          }
        }
      });
      inputTimingObserver.observe({
        type: "event",
        buffered: true,
        durationThreshold: 16,
      });
    }
    let previousFrame = null;
    function recordFrame(now) {
      if (previousFrame !== null) {
        window.__wasmFrameIntervals.push(now - previousFrame);
      }
      previousFrame = now;
      window.requestAnimationFrame(recordFrame);
    }
    window.requestAnimationFrame(recordFrame);
  });
  if (process.env.WASM_FORCE_CPU_RENDERER === "1") {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "gpu", {
        configurable: true,
        value: undefined,
      });
    });
  }

  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  await page.locator("#tools-panel > summary").click();
  await page.locator("#import-files-picker").setInputFiles([
    {
      name: "3      ASCII   PROG WASM vector loop",
      mimeType: "application/octet-stream",
      buffer: Buffer.from(vectorProgram),
    },
    {
      name: "4      ASCII   PROG WASM integer arithmetic",
      mimeType: "application/octet-stream",
      buffer: Buffer.from(integerArithmeticProgram),
    },
    {
      name: "5      ASCII   PROG WASM floating arithmetic",
      mimeType: "application/octet-stream",
      buffer: Buffer.from(floatingPointArithmeticProgram),
    },
  ]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(3);

  await page.locator("#run-pause").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );

  const summary = page.locator("#workload-summary");
  const rows = [];
  await measureScenario(page, summary, "BASIC prompt/cursor", rows);
  const inputRows = await measureInputEventDelay(page, summary);

  await page.locator("#speed").selectOption("0");
  await page.locator("#reset").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );

  const screen = page.locator("#display-surface > canvas:not([hidden])");
  const priorFrame = await screen.screenshot();
  await expect(summary).toContainText("x1 | average");
  await launchFlashProgram(page, 3);
  await expect.poll(async () => !(await screen.screenshot()).equals(priorFrame), {
    timeout: 10_000,
  }).toBe(true);
  await measureScenario(page, summary, "BASIC vector loop", rows);

  await page.locator("#speed").selectOption("0");
  await page.locator("#reset").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );
  await launchFlashProgram(page, 4);
  await waitForFramebufferPoints(
    page,
    [[512, 390], [512, 400], [512, 410]],
    "Integer",
  );
  await measureScenario(page, summary, "BASIC integer arithmetic", rows);

  await page.locator("#speed").selectOption("0");
  await page.locator("#reset").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );
  await launchFlashProgram(page, 5);
  await waitForFramebufferPoints(
    page,
    [[512, 390], [512, 400], [512, 410]],
    "Floating-point",
  );
  await measureScenario(page, summary, "BASIC floating-point arithmetic", rows);

  const environment = await page.evaluate(() => ({
    userAgent: navigator.userAgent,
    hardwareConcurrency: navigator.hardwareConcurrency,
    devicePixelRatio: window.devicePixelRatio,
    viewport: `${window.innerWidth}x${window.innerHeight}`,
  }));
  const renderer = await page.locator("#renderer-status").getAttribute("data-renderer");
  const report = [
    "Tektronix 4051 WASM performance benchmark",
    `Measured: ${new Date().toISOString()}`,
    `Browser: ${environment.userAgent}`,
    `Logical processors: ${environment.hardwareConcurrency ?? "unknown"}`,
    `Viewport: ${environment.viewport} (devicePixelRatio ${environment.devicePixelRatio})`,
    `Renderer: ${renderer ?? "unknown"}`,
    `Method: rolling ${workloadWindowSeconds.toFixed(1)} second sample window;`,
    "emulator.advance() wall time as a percentage of one 10 ms budget.",
    "This measures emulation-call cost, not total browser CPU utilization.",
    "",
    "Workload | Speed | Average budget use | Peak budget use | Average frame (ms) | P95 frame (ms) | Peak frame (ms) | Revision changes | Presented revisions | Presenter calls | Mean presenter (ms) | Peak presenter (ms) | Skipped presentations | Surface acquisitions | Mean acquire (ms) | Peak acquire (ms) | Samples | Window (s)",
    ...rows.map((row) => (
      `${row.workload} | ${row.speed} | ${row.averagePercent.toFixed(2)}% | `
      + `${row.peakPercent.toFixed(2)}% | ${row.frameAverageMs.toFixed(2)} | `
      + `${row.frameP95Ms.toFixed(2)} | ${row.framePeakMs.toFixed(2)} | `
      + `${row.revisionChanges} | ${row.presentedRevisions} | `
      + `${row.presenterCalls} | ${row.presenterMeanMs.toFixed(3)} | `
      + `${row.presenterPeakMs.toFixed(3)} | ${row.skippedPresentations} | `
      + `${row.surfaceAcquisitions?.toString() ?? "n/a"} | `
      + `${row.surfaceMeanMs?.toFixed(3) ?? "n/a"} | `
      + `${row.surfacePeakMs?.toFixed(3) ?? "n/a"} | `
      + `${row.sampleCount} | ${row.historySeconds.toFixed(2)}`
    )),
    "Presenter duration is the synchronous present_webgpu() wall time; surface acquisition duration is a subset.",
    "",
    "Input event timing (samples only when browser Event Timing reports duration >=16 ms)",
    "Speed | Playwright press wall (ms) | Max page-to-key-handler delay (ms) | Max Event Timing input delay (ms) | Max event duration (ms) | Key / Event Timing entries",
    ...inputRows.map((row) => (
      `${row.speed} | ${row.pressWallMs.toFixed(2)} | `
      + `${row.maxDispatchDelayMs?.toFixed(2) ?? "none"} | `
      + `${row.maxInputDelayMs?.toFixed(2) ?? "none"} | `
      + `${row.maxDurationMs?.toFixed(2) ?? "none"} | `
      + `${row.keyEventCount} / ${row.entryCount}`
    )),
    "Page-to-key-handler delay is measured from a page timestamp immediately before Playwright sends the key.",
    "Event input delay is processingStart - startTime; Playwright wall time includes browser/tool scheduling.",
    "",
    "No fixed pass/fail performance threshold is applied; results depend on the host.",
  ].join("\n");
  const reportPath = testInfo.outputPath("wasm-performance.txt");
  await writeFile(reportPath, report);
  await testInfo.attach("WASM performance results", {
    path: reportPath,
    contentType: "text/plain",
  });
  console.log(report);
});

registerProfileTest("WebGPU", false);
registerProfileTest("CPU fallback", true);

test("measures x100 WebGPU segment-buffer costs", async ({ page }, testInfo) => {
  test.skip(
    process.env.RUN_WASM_BUFFER_METRICS !== "1",
    "Set RUN_WASM_BUFFER_METRICS=1 to measure WebGPU segment-buffer host costs.",
  );
  test.setTimeout(60_000);
  await installWebGpuBufferMetrics(page);

  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  const renderer = await page.locator("#renderer-status").getAttribute("data-renderer");
  test.skip(renderer !== "webgpu", "WebGPU is unavailable in this browser.");
  await page.locator("#speed").selectOption("6");
  await page.locator("#run-pause").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );

  const initialMetrics = await readWebGpuBufferMetrics(page);
  expect(initialMetrics.available).toBe(true);
  await resetWebGpuBufferMetrics(page);
  await page.waitForTimeout(5_000);
  await page.locator("#run-pause").click();
  const queueDrainMilliseconds = await measureWebGpuQueueDrain(page);
  const metrics = await readWebGpuBufferMetrics(page);

  expect(metrics.elapsedMilliseconds).toBeGreaterThanOrEqual(5_000);
  expect(Number.isFinite(queueDrainMilliseconds)).toBe(true);
  expect(metrics.segmentBufferCreateCalls).toBeGreaterThan(0);
  expect(metrics.segmentBufferInitializeCalls).toBe(metrics.segmentBufferCreateCalls);
  expect(metrics.segmentBufferBytes).toBeGreaterThan(0);
  expect(metrics.queueSubmitCalls).toBeGreaterThan(0);
  const report = [
    `WebGPU segment-buffer host timing: x100 BASIC prompt/cursor, ${(metrics.elapsedMilliseconds / 1_000).toFixed(2)} seconds`,
    `createBuffer: ${metrics.segmentBufferCreateCalls} calls, ${metrics.segmentBufferBytes} bytes, ${metrics.segmentBufferCreateMilliseconds.toFixed(3)} ms total, ${metrics.segmentBufferPeakCreateMilliseconds.toFixed(3)} ms peak`,
    `mapped initialization through unmap: ${metrics.segmentBufferInitializeCalls} calls, ${metrics.segmentBufferInitializeMilliseconds.toFixed(3)} ms total, ${metrics.segmentBufferPeakInitializeMilliseconds.toFixed(3)} ms peak`,
    `queue.submit: ${metrics.queueSubmitCalls} calls, ${metrics.queueSubmitMilliseconds.toFixed(3)} ms total, ${metrics.queueSubmitPeakMilliseconds.toFixed(3)} ms peak`,
    `queue completion after pause: ${queueDrainMilliseconds.toFixed(3)} ms`,
    "Create, mapped initialization, and submit values are synchronous host-side times; queue completion is the post-pause wait for outstanding GPU work.",
  ].join("\n");
  const reportPath = testInfo.outputPath("webgpu-segment-buffer-metrics.txt");
  await writeFile(reportPath, report);
  await testInfo.attach("WebGPU segment-buffer host metrics", {
    path: reportPath,
    contentType: "text/plain",
  });
  console.log(report);
});

function registerProfileTest(workloadRenderer, forceCpuRenderer) {
  test(`captures a ${workloadRenderer} profile of the x100 BASIC prompt`, async ({ page }, testInfo) => {
    test.skip(
      process.env.RUN_WASM_CPU_PROFILE !== "1",
      "Set RUN_WASM_CPU_PROFILE=1 to capture an opt-in Chromium CPU profile.",
    );
    test.setTimeout(60_000);
    await captureWasmProfile(page, testInfo, workloadRenderer, forceCpuRenderer);
  });
}

async function captureWasmProfile(page, testInfo, workloadRenderer, forceCpuRenderer) {
  if (forceCpuRenderer) {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "gpu", {
        configurable: true,
        value: undefined,
      });
    });
  }

  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  const renderer = await page.locator("#renderer-status").getAttribute("data-renderer");
  test.skip(
    renderer !== (forceCpuRenderer ? "cpu-fallback" : "webgpu"),
    "The requested renderer is unavailable in this browser.",
  );
  await page.locator("#speed").selectOption("6");
  await page.locator("#run-pause").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );

  const profiler = await page.context().newCDPSession(page);
  await profiler.send("Profiler.enable");
  await profiler.send("Profiler.start");
  await page.waitForTimeout(5_000);
  const { profile } = await profiler.send("Profiler.stop");
  await profiler.detach();

  expect(profile.samples.length).toBeGreaterThan(0);
  const nodes = new Map(profile.nodes.map((node) => [node.id, node]));
  const parents = new Map();
  for (const node of profile.nodes) {
    for (const childId of node.children ?? []) {
      parents.set(childId, node.id);
    }
  }
  const wasmFrames = new Map();
  for (const [sampleIndex, sampleId] of profile.samples.entries()) {
    let node = nodes.get(sampleId);
    while (node) {
      const { functionName, url } = node.callFrame;
      if (/wasm/i.test(functionName) || /\.wasm(?:$|[?#])/i.test(url)) {
        const key = `${functionName || "(anonymous)"} - ${url || "(no URL)"}`;
        const current = wasmFrames.get(key) ?? { samples: 0, milliseconds: 0 };
        current.samples += 1;
        current.milliseconds += (profile.timeDeltas?.[sampleIndex] ?? 0) / 1000;
        wasmFrames.set(key, current);
      }
      node = nodes.get(parents.get(node.id));
    }
  }
  const summary = [
    `Chrome CPU profile: ${workloadRenderer} WASM BASIC prompt/cursor at x100`,
    "Sampling duration: 5 seconds; profiler overhead makes this unsuitable for timing comparisons.",
    `Samples: ${profile.samples.length}`,
    "Inclusive WASM frames (sampled milliseconds):",
    ...[...wasmFrames.entries()]
      .sort((left, right) => right[1].milliseconds - left[1].milliseconds)
      .slice(0, 30)
      .map(([frame, timing]) => (
        `${timing.milliseconds.toFixed(1)} ms | ${timing.samples} samples | ${frame}`
      )),
    wasmFrames.size === 0 ? "No WASM frames were named; inspect the attached raw profile." : "",
  ].filter(Boolean).join("\n");
  const profileName = `wasm-x100-basic-${forceCpuRenderer ? "cpu" : "webgpu"}`;
  const profilePath = testInfo.outputPath(`${profileName}.cpuprofile`);
  await writeFile(profilePath, JSON.stringify(profile));
  await testInfo.attach(`WASM x100 ${workloadRenderer} profile`, {
    path: profilePath,
    contentType: "application/json",
  });
  const summaryPath = testInfo.outputPath(`${profileName}-profile.txt`);
  await writeFile(summaryPath, summary);
  await testInfo.attach(`WASM x100 ${workloadRenderer} profile summary`, {
    path: summaryPath,
    contentType: "text/plain",
  });
  console.log(summary);
}

async function measureInputEventDelay(page, summary) {
  const selectedSpeeds = speeds.filter(({ index }) => ["0", "4", "5", "6"].includes(index));
  const rows = [];
  for (const speed of selectedSpeeds) {
    await page.locator("#speed").selectOption(speed.index);
    await expect.poll(
      async () => (await summary.textContent())?.split(" | ", 1)[0],
    ).toBe(speed.label);
    await page.evaluate(() => {
      window.__wasmInputTimings.length = 0;
      window.__wasmKeyDispatchTimings.length = 0;
    });
    await page.locator("#display-surface").click();
    await page.evaluate(() => {
      window.__wasmKeySentAt = performance.now();
    });
    const startedAt = performance.now();
    await page.keyboard.press("a");
    const pressWallMs = performance.now() - startedAt;
    await page.waitForTimeout(100);
    const { timings, dispatchTimings } = await page.evaluate(() => ({
      timings: window.__wasmInputTimings,
      dispatchTimings: window.__wasmKeyDispatchTimings,
    }));
    rows.push({
      speed: speed.label,
      pressWallMs,
      maxDispatchDelayMs: dispatchTimings.length > 0
        ? Math.max(...dispatchTimings.map((timing) => timing.delayMs))
        : null,
      keyEventCount: dispatchTimings.length,
      maxInputDelayMs: timings.length > 0
        ? Math.max(...timings.map((timing) => timing.inputDelayMs))
        : null,
      maxDurationMs: timings.length > 0
        ? Math.max(...timings.map((timing) => timing.durationMs))
        : null,
      entryCount: timings.length,
    });
  }
  return rows;
}

async function measureScenario(page, summary, workload, rows) {
  const speedControl = page.locator("#speed");
  for (const speed of speeds) {
    const alternateIndex = speed.index === "0" ? "1" : "0";
    const alternateLabel = speeds[Number(alternateIndex)].label;
    await speedControl.selectOption(alternateIndex);
    await expect.poll(
      async () => (await summary.textContent())?.split(" | ", 1)[0],
    ).toBe(alternateLabel);
    await speedControl.selectOption(speed.index);
    await resetWebGpuSurfaceMetrics(page);
    await page.evaluate(() => {
      window.__wasmFrameIntervals.length = 0;
    });
    await expect.poll(
      async () => {
        const label = (await summary.textContent())?.split(" | ", 1)[0];
        const history = Number(await summary.getAttribute("data-history-seconds"));
        return label === speed.label && history >= workloadWindowSeconds;
      },
      { timeout: 15_000 },
    ).toBe(true);

    const measurement = await summary.evaluate((element) => ({
      averagePercent: Number(element.dataset.averagePercent),
      peakPercent: Number(element.dataset.peakPercent),
      sampleCount: Number(element.dataset.sampleCount),
      historySeconds: Number(element.dataset.historySeconds),
    }));
    const frameIntervals = await page.evaluate(() => window.__wasmFrameIntervals);
    const surfaceMetrics = await readWebGpuSurfaceMetrics(page);
    const sortedFrameIntervals = [...frameIntervals].sort((a, b) => a - b);
    const frameAverageMs = frameIntervals.reduce((sum, interval) => sum + interval, 0)
      / frameIntervals.length;
    expect(Number.isFinite(measurement.averagePercent)).toBe(true);
    expect(Number.isFinite(measurement.peakPercent)).toBe(true);
    expect(Number.isFinite(frameAverageMs)).toBe(true);
    rows.push({
      workload,
      speed: speed.label,
      ...measurement,
      frameAverageMs,
      frameP95Ms: sortedFrameIntervals[Math.floor(sortedFrameIntervals.length * 0.95)],
      framePeakMs: sortedFrameIntervals.at(-1),
      presenterCalls: surfaceMetrics.presentationCalls,
      presenterMeanMs: surfaceMetrics.presentationCalls > 0
        ? surfaceMetrics.totalPresentationMilliseconds / surfaceMetrics.presentationCalls
        : 0,
      presenterPeakMs: surfaceMetrics.peakPresentationMilliseconds,
      skippedPresentations: surfaceMetrics.skippedPresentations,
      revisionChanges: surfaceMetrics.revisionChanges,
      presentedRevisions: surfaceMetrics.presentedRevisions,
      surfaceAcquisitions: surfaceMetrics.available ? surfaceMetrics.acquisitions : null,
      surfaceMeanMs: surfaceMetrics.available && surfaceMetrics.acquisitions > 0
        ? surfaceMetrics.totalMilliseconds / surfaceMetrics.acquisitions
        : surfaceMetrics.available ? 0 : null,
      surfacePeakMs: surfaceMetrics.available ? surfaceMetrics.peakMilliseconds : null,
    });
  }
}

