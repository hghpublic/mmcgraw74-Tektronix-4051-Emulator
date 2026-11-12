import { expect, test } from "@playwright/test";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";

async function loadDisplay(page) {
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  const surface = page.locator("#display-surface");
  const canvas = page.locator("#display-surface > canvas:not([hidden])");
  await expect(canvas).toBeVisible();
  return { surface, canvas };
}

test("zooms around the pointer, pans with left drag, and resets with right click", async ({ page }) => {
  const { surface, canvas } = await loadDisplay(page);
  const initial = await canvas.boundingBox();
  expect(initial).not.toBeNull();
  const pointer = {
    x: initial.x + initial.width * 0.72,
    y: initial.y + initial.height * 0.31,
  };
  const anchorBefore = {
    x: (pointer.x - initial.x) / initial.width,
    y: (pointer.y - initial.y) / initial.height,
  };

  await page.mouse.move(pointer.x, pointer.y);
  await page.mouse.wheel(0, -120);
  const zoomed = await canvas.boundingBox();
  expect(zoomed.width).toBeGreaterThan(initial.width);
  expect((pointer.x - zoomed.x) / zoomed.width).toBeCloseTo(anchorBefore.x, 2);
  expect((pointer.y - zoomed.y) / zoomed.height).toBeCloseTo(anchorBefore.y, 2);

  const surfaceBounds = await surface.boundingBox();
  const dragStart = {
    x: surfaceBounds.x + surfaceBounds.width / 2,
    y: surfaceBounds.y + surfaceBounds.height / 2,
  };
  const dragDelta = { x: 48.5, y: 27.25 };
  await page.mouse.move(dragStart.x, dragStart.y);
  await page.mouse.down({ button: "left" });
  await page.mouse.move(dragStart.x + dragDelta.x, dragStart.y + dragDelta.y, { steps: 3 });
  await page.mouse.up({ button: "left" });
  const panned = await canvas.boundingBox();
  expect(panned.x - zoomed.x).toBeCloseTo(dragDelta.x, 1);
  expect(panned.y - zoomed.y).toBeCloseTo(dragDelta.y, 1);

  await page.mouse.click(dragStart.x, dragStart.y, { button: "right" });
  const reset = await canvas.boundingBox();
  expect(Math.abs(reset.width - initial.width)).toBeLessThan(1);
  expect(Math.abs(reset.height - initial.height)).toBeLessThan(1);
  expect(Math.abs(reset.x - initial.x)).toBeLessThan(1);
  expect(Math.abs(reset.y - initial.y)).toBeLessThan(1);
});

test("keeps zoom within 0.25× and 8× the fitted display size", async ({ page }) => {
  const { canvas } = await loadDisplay(page);
  const initial = await canvas.boundingBox();
  const center = {
    x: initial.x + initial.width / 2,
    y: initial.y + initial.height / 2,
  };
  await page.mouse.move(center.x, center.y);

  await page.mouse.wheel(0, 10000);
  let current = await canvas.boundingBox();
  expect(current.width).toBeCloseTo(initial.width * 0.25, 0);
  await page.mouse.wheel(0, 10000);
  current = await canvas.boundingBox();
  expect(current.width).toBeCloseTo(initial.width * 0.25, 0);

  await page.mouse.wheel(0, -10000);
  current = await canvas.boundingBox();
  expect(current.width).toBeCloseTo(initial.width * 8, 0);
  await page.mouse.wheel(0, -10000);
  current = await canvas.boundingBox();
  expect(current.width).toBeCloseTo(initial.width * 8, 0);
});

test("exposes logical camera transforms and avoids double CSS zoom in vector presentation", async ({ page }) => {
  await page.goto(shellPath);
  const setup = await page.evaluate(async () => {
    const { bindDisplayCamera } = await import("./display_camera.js");
    const surface = document.createElement("div");
    Object.assign(surface.style, {
      position: "fixed",
      left: "24px",
      top: "24px",
      width: "400px",
      height: "200px",
    });
    const cpuCanvas = document.createElement("canvas");
    cpuCanvas.id = "screen";
    cpuCanvas.width = 1000;
    cpuCanvas.height = 500;
    Object.assign(cpuCanvas.style, { width: "100%", height: "100%" });
    const webGpuCanvas = document.createElement("canvas");
    webGpuCanvas.id = "screen-webgpu";
    webGpuCanvas.width = 1000;
    webGpuCanvas.height = 500;
    Object.assign(webGpuCanvas.style, { width: "100%", height: "100%" });
    surface.append(cpuCanvas, webGpuCanvas);
    document.body.append(surface);
    const updates = [];
    const camera = bindDisplayCamera(surface, (transform) => updates.push(transform));
    window.__displayCameraContractTest = { camera, cpuCanvas, webGpuCanvas, updates };
    const bounds = surface.getBoundingClientRect();
    return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
  });

  await page.mouse.move(setup.x, setup.y);
  await page.mouse.wheel(0, -120);
  await page.mouse.move(setup.x, setup.y);
  await page.mouse.down({ button: "left" });
  await page.mouse.move(setup.x + 40, setup.y + 20);
  await page.mouse.up({ button: "left" });

  const result = await page.evaluate(() => {
    const { camera, cpuCanvas, webGpuCanvas, updates } = window.__displayCameraContractTest;
    const transform = camera.getViewTransform();
    const widthScale = webGpuCanvas.width / webGpuCanvas.clientWidth;
    const heightScale = webGpuCanvas.height / webGpuCanvas.clientHeight;
    camera.setVectorPresentation(true);
    return {
      transform,
      widthScale,
      heightScale,
      cpuCssTransform: cpuCanvas.style.transform,
      webGpuCssTransform: webGpuCanvas.style.transform,
      lastUpdate: updates.at(-1),
    };
  });
  expect(result.transform.zoom).toBeGreaterThan(1);
  expect(result.transform.panX).toBeCloseTo(40 * result.widthScale, 0);
  expect(result.transform.panY).toBeCloseTo(20 * result.heightScale, 0);
  expect(result.cpuCssTransform).not.toBe("");
  expect(result.webGpuCssTransform).toBe("");
  expect(result.lastUpdate).toEqual(result.transform);
});
