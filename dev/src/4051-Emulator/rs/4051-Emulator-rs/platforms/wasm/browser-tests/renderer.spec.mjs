import { expect, test } from "@playwright/test";
import {
  installWebGpuSurfaceMetrics,
  readWebGpuSurfaceMetrics,
  resetWebGpuSurfaceMetrics,
} from "./webgpu-surface-metrics.mjs";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";

test("presents with WebGPU when available and exposes CPU fallback otherwise", async ({ page }) => {
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const rendererStatus = page.locator("#renderer-status");
  const renderer = await rendererStatus.getAttribute("data-renderer");
  expect(["webgpu", "extended-webgpu", "cpu", "cpu-fallback"]).toContain(renderer);
  await expect(page.locator("#display-surface > canvas:not([hidden])")).toHaveCount(1);

  if (renderer === "webgpu") {
    await expect(rendererStatus).toHaveText("WebGPU");
    await expect(page.locator("#screen-webgpu")).toBeVisible();
    await expect(page.locator("#screen")).toBeHidden();
  } else {
    await expect(page.locator("#screen")).toBeVisible();
    await expect(page.locator("#screen-webgpu")).toBeHidden();
    if (renderer === "cpu-fallback") {
      await expect(rendererStatus).toHaveAttribute("title", /WebGPU/);
    }
  }
});

test("selects the Extended WebGPU renderer only when requested", async ({ page }) => {
  await page.goto(`${shellPath}?renderer=extended`);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const rendererStatus = page.locator("#renderer-status");
  const renderer = await rendererStatus.getAttribute("data-renderer");
  test.skip(renderer !== "extended-webgpu", "This browser does not initialize WebGPU.");
  await expect(rendererStatus).toHaveText("Extended WebGPU");
  await expect(page.locator("#screen-webgpu")).toBeVisible();
  await expect(page.locator("#screen")).toBeHidden();
});

test("enables vector scene capture only on Extended WebGPU and defaults off", async ({ page }) => {
  await page.goto(`${shellPath}?renderer=extended`);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const rendererStatus = page.locator("#renderer-status");
  test.skip(
    (await rendererStatus.getAttribute("data-renderer")) !== "extended-webgpu",
    "This browser does not initialize WebGPU.",
  );
  await page.locator("#tools-panel > summary").click();
  await page.locator("#test-tab").click();
  const toggle = page.locator("#vector-zoom-enabled");
  const mode = page.locator("#vector-zoom-mode");
  await expect(page.locator("#vector-zoom-mode-control")).toBeHidden();
  await expect(toggle).toBeEnabled();
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect(toggle).toBeChecked();
  await expect(mode).toBeVisible();
  await expect(mode).toHaveValue("raster");
  await expect(page.locator("#vector-zoom-status"))
    .toHaveText(
      "Vector scene capture is enabled in Raster mode; select Vector in the menu to use high-definition zoom.",
    );
  await expect(rendererStatus).toHaveText("Extended WebGPU");

  const canvas = page.locator("#screen-webgpu");
  const bounds = await canvas.boundingBox();
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.mouse.wheel(0, -120);
  await expect.poll(() => canvas.evaluate((element) => getComputedStyle(element).transform))
    .not.toBe("none");
  await mode.selectOption("vector");
  await expect.poll(() => canvas.evaluate((element) => getComputedStyle(element).transform))
    .toBe("none");
  await expect(mode).toHaveValue("vector");
  await mode.selectOption("raster");
  await expect.poll(() => canvas.evaluate((element) => getComputedStyle(element).transform))
    .not.toBe("none");
  await expect(mode).toHaveValue("raster");

  await toggle.uncheck();
  await expect(toggle).not.toBeChecked();
  await expect(page.locator("#vector-zoom-mode-control")).toBeHidden();
  await expect(page.locator("#vector-zoom-status")).toHaveText("Vector scene capture is disabled.");
});

test("reports the CPU renderer when the browser lacks the WebGPU API", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      value: undefined,
    });
  });
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const rendererStatus = page.locator("#renderer-status");
  await expect(rendererStatus).toHaveAttribute("data-renderer", "cpu-fallback");
  await expect(rendererStatus).toHaveText("CPU fallback");
  await expect(rendererStatus).toHaveAttribute("title", /WebGPU/);
  await expect(page.locator("#screen")).toBeVisible();
  await expect(page.locator("#screen-webgpu")).toBeHidden();
  await page.locator("#tools-panel > summary").click();
  await page.locator("#test-tab").click();
  await expect(page.locator("#vector-zoom-enabled")).toBeDisabled();
  await expect(page.locator("#vector-zoom-enabled")).not.toBeChecked();
  await expect(page.locator("#vector-zoom-mode-control")).toBeHidden();
});

test("falls back to the CPU renderer when WebGPU adapter initialization fails", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      value: {
        requestAdapter: async () => {
          throw new Error("test adapter unavailable");
        },
      },
    });
  });
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const rendererStatus = page.locator("#renderer-status");
  await expect(rendererStatus).toHaveAttribute("data-renderer", "cpu-fallback");
  await expect(rendererStatus).toHaveText("CPU fallback");
  await expect(rendererStatus).toHaveAttribute("title", /WebGPU/);
  await expect(page.locator("#screen")).toBeVisible();
  await expect(page.locator("#screen-webgpu")).toBeHidden();
});

test("reuses unchanged WebGPU frames and redraws after viewport resize", async ({ page }) => {
  await installWebGpuSurfaceMetrics(page);
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const renderer = await page.locator("#renderer-status").getAttribute("data-renderer");
  test.skip(renderer !== "webgpu", "This browser does not initialize WebGPU.");
  const metrics = await readWebGpuSurfaceMetrics(page);
  test.skip(!metrics.available, "This browser does not expose a measurable WebGPU canvas.");

  await expect(page.locator("#run-pause")).toHaveText("Start");
  expect(metrics.presentedRevisions).toBeGreaterThan(0);
  await expect.poll(async () => (await readWebGpuSurfaceMetrics(page)).acquisitions)
    .toBeGreaterThan(0);
  await resetWebGpuSurfaceMetrics(page);
  await page.locator("#run-pause").click();
  await expect(page.locator("#run-pause")).toHaveText("Pause");
  await expect.poll(async () => (await readWebGpuSurfaceMetrics(page)).revisionChanges)
    .toBeGreaterThan(0);
  await expect.poll(async () => (await readWebGpuSurfaceMetrics(page)).presentedRevisions)
    .toBeGreaterThan(0);
  await page.locator("#run-pause").click();
  await expect(page.locator("#run-pause")).toHaveText("Start");

  await resetWebGpuSurfaceMetrics(page);
  const canvas = page.locator("#screen-webgpu");
  const firstFrame = await canvas.screenshot();
  await page.waitForTimeout(200);

  const idleMetrics = await readWebGpuSurfaceMetrics(page);
  expect(idleMetrics.acquisitions).toBe(0);
  expect(idleMetrics.presentationCalls).toBe(0);
  expect(idleMetrics.skippedPresentations).toBeGreaterThan(0);
  expect(idleMetrics.revisionChanges).toBe(0);
  expect(await canvas.screenshot()).toEqual(firstFrame);

  await resetWebGpuSurfaceMetrics(page);
  await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await expect.poll(async () => (await readWebGpuSurfaceMetrics(page)).presentationCalls)
    .toBeGreaterThan(0);
  expect((await readWebGpuSurfaceMetrics(page)).acquisitions).toBeGreaterThan(0);
  expect(await canvas.screenshot()).toEqual(firstFrame);

  await resetWebGpuSurfaceMetrics(page);
  await page.evaluate(() => window.dispatchEvent(new Event("pageshow")));
  await expect.poll(async () => (await readWebGpuSurfaceMetrics(page)).presentationCalls)
    .toBeGreaterThan(0);
  expect((await readWebGpuSurfaceMetrics(page)).acquisitions).toBeGreaterThan(0);

  await page.setViewportSize({ width: 1281, height: 721 });
  await expect.poll(async () => (await readWebGpuSurfaceMetrics(page)).presentationCalls)
    .toBeGreaterThan(0);
  expect((await readWebGpuSurfaceMetrics(page)).acquisitions).toBeGreaterThan(0);
});
