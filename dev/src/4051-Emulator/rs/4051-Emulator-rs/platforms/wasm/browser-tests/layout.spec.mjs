import { expect, test } from "@playwright/test";
import {
  workloadPointCoordinates,
  workloadSampleSegments,
  retainWorkloadSample,
} from "../browser/workload_graph.js";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";
const visibleDisplayCanvas = "#display-surface > canvas:not([hidden])";

test("keeps tools collapsed and switches between all five tool tabs", async ({ page }) => {
  await page.goto(shellPath);
  const start = page.locator("#run-pause");
  await expect(start).toBeEnabled();
  await expect(start).toHaveText("Start");
  await expect(start).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator("#runtime-status, #machine-status")).toHaveCount(0);

  const menuOrder = await page.locator(".menu-bar > *").evaluateAll((items) => (
    items
      .filter((item) => !item.hidden)
      .map((item) => item.id || item.querySelector("select")?.id)
  ));
  expect(menuOrder).toEqual(["run-pause", "reset", "speed"]);
  const speedFontSize = await page.locator("#speed").evaluate((select) => (
    Number.parseFloat(getComputedStyle(select).fontSize)
  ));
  const normalFontSize = await page.locator("html").evaluate((html) => (
    Number.parseFloat(getComputedStyle(html).fontSize)
  ));
  expect(speedFontSize).toBeLessThan(normalFontSize);

  const tools = page.locator("#tools-panel");
  await expect(tools).not.toHaveAttribute("open", "");
  await expect(tools).toHaveCSS("align-self", "start");
  await expect(page.locator("#controls-panel")).toBeHidden();
  const desktopCanvas = await page.locator(visibleDisplayCanvas).boundingBox();
  expect(desktopCanvas).not.toBeNull();
  expect(desktopCanvas.width / desktopCanvas.height).toBeCloseTo(1046 / 830, 2);
  for (const [id, label] of [
    ["#status-busy", "BUSY"],
    ["#status-break", "BREAK"],
    ["#status-io", "I/O"],
    ["#status-power", "POWER"],
  ]) {
    await expect(page.locator(id)).toHaveAttribute("aria-label", `${label} off`);
  }

  await tools.locator("> summary").click();
  const controls = page.locator("#controls-panel");
  await page.locator("#controls-tab").click();
  await expect(controls).toBeVisible();
  await expect(page.locator("#flash-drive-panel")).toBeHidden();
  await expect(page.locator("#editor-panel")).toBeHidden();
  await expect(page.locator("#workload-panel")).toBeHidden();
  await expect(controls.locator("#run-pause, #reset, #speed")).toHaveCount(0);
  await start.click();
  await expect(start).toHaveText("Pause");
  await expect(start).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#status-power")).toHaveAttribute("aria-label", "POWER on");
  await start.click();
  await expect(start).toHaveText("Start");
  await expect(start).toHaveAttribute("aria-pressed", "false");
  await page.locator("#reset").click();
  await page.locator("#speed").selectOption("6");
  await expect(page.locator("#speed")).toHaveValue("6");
  await page.locator("#speed").selectOption("0");

  await page.locator("#flash-drive-tab").click();
  await expect(controls).toBeHidden();
  await expect(page.locator("#flash-drive-panel")).toBeVisible();
  await expect(page.locator("#workload-panel")).toBeHidden();

  await page.locator("#controls-tab").focus();
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator("#editor-tab")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#editor-panel")).toBeVisible();
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator("#flash-drive-tab")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("End");
  await expect(page.locator("#test-tab")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#test-panel")).toBeVisible();
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator("#workload-tab")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#workload-panel")).toBeVisible();
  await expect(page.locator("#flash-drive-panel")).toBeHidden();
  await expect(controls).toBeHidden();
});

test("keeps the directory on the right as a collapsible narrow-screen panel", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const tools = page.locator("#tools-panel");
  await expect(tools).not.toHaveAttribute("open", "");
  await tools.locator("> summary").click();
  await expect(tools).toHaveAttribute("open", "");
  await expect(tools).toHaveCSS("position", "fixed");

  const panelBox = await tools.boundingBox();
  const canvasBox = await page.locator(visibleDisplayCanvas).boundingBox();
  expect(panelBox).not.toBeNull();
  expect(canvasBox).not.toBeNull();
  expect(Math.abs(390 - panelBox.x - panelBox.width - 8)).toBeLessThan(1);
  expect(canvasBox.width).toBeGreaterThan(250);
  expect(canvasBox.width / canvasBox.height).toBeCloseTo(1046 / 830, 2);
});

test("shows user-definable keys in numeric order in a four-by-five grid", async ({ page }) => {
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  await page.locator("#tools-panel > summary").click();
  await page.locator("#controls-tab").click();

  const frontPanel = page.getByRole("region", { name: "Front panel" });
  const groups = frontPanel.getByRole("group");
  await expect(groups).toHaveCount(3);
  expect(await groups.evaluateAll((items) => items.map((item) => (
    item.querySelector("h3").textContent.trim()
  )))).toEqual(["User Definable", "Line Editor", "Program and special"]);
  const frontPanelContentWidth = await frontPanel.evaluate((panel) => {
    const style = getComputedStyle(panel);
    return panel.clientWidth - Number.parseFloat(style.paddingLeft) - Number.parseFloat(style.paddingRight);
  });
  const groupBounds = await groups.evaluateAll((items) => items.map((item) => {
    const { left, top, width, height } = item.getBoundingClientRect();
    return { left, top, width, height };
  }));
  expect(groupBounds[0].width).toBeCloseTo(frontPanelContentWidth, 0);
  expect(groupBounds.every((bounds) => Math.abs(bounds.width - groupBounds[0].width) < 1)).toBe(true);
  for (let index = 1; index < groupBounds.length; index += 1) {
    expect(groupBounds[index].top).toBeGreaterThanOrEqual(
      groupBounds[index - 1].top + groupBounds[index - 1].height,
    );
  }

  const group = frontPanel.getByRole("group", { name: "User Definable" });
  await expect(group).toBeVisible();
  const buttons = group.getByRole("button");
  await expect(buttons).toHaveCount(20);
  expect(await buttons.allTextContents()).toEqual(
    Array.from({ length: 20 }, (_, index) => String(index + 1)),
  );
  expect(await buttons.evaluateAll((items) => items.map((button) => (
    [button.dataset.tekCode, button.dataset.tekShift]
  )))).toEqual([
    ...Array.from({ length: 10 }, (_, index) => [
      `0x${(0x60 + index).toString(16)}`,
      "false",
    ]),
    ...Array.from({ length: 10 }, (_, index) => [
      `0x${(0x60 + index).toString(16)}`,
      "true",
    ]),
  ]);

  const positions = await buttons.evaluateAll((items) => items.map((button) => {
    const { left, top } = button.getBoundingClientRect();
    return [Math.round(left), Math.round(top)];
  }));
  expect(new Set(positions.map(([, top]) => top)).size).toBe(4);
  expect(new Set(positions.map(([left]) => left)).size).toBe(5);
  for (let row = 0; row < 4; row += 1) {
    expect(new Set(positions.slice(row * 5, row * 5 + 5).map(([, top]) => top)).size).toBe(1);
  }

  const lineEditor = frontPanel.getByRole("group", { name: "Line Editor" });
  const lineButtons = lineEditor.getByRole("button");
  await expect(lineButtons).toHaveCount(10);
  expect(await lineButtons.evaluateAll((items) => items.map((button) => (
    button.getAttribute("aria-label") ?? button.textContent.trim()
  )))).toEqual([
    "Compress",
    "Rub Out Left",
    "Rub Out Right",
    "Reprint",
    "Recall Next Line",
    "Expand",
    "Back Space",
    "Space",
    "Clear",
    "Recall Line",
  ]);
  expect(await lineEditor.getByRole("button", { name: "Recall Next Line" })
    .evaluate((button) => button.innerText.trim())).toBe("Recall\nNext Line");
  expect(await lineButtons.evaluateAll((items) => items.map((button) => (
    [button.dataset.tekCode, button.dataset.tekShift]
  )))).toEqual([
    ...Array.from({ length: 5 }, (_, index) => [
      `0x${(0x70 + index).toString(16)}`,
      "true",
    ]),
    ...Array.from({ length: 5 }, (_, index) => [
      `0x${(0x70 + index).toString(16)}`,
      "false",
    ]),
  ]);
  const linePositions = await lineButtons.evaluateAll((items) => items.map((button) => {
    const { left, top } = button.getBoundingClientRect();
    return [Math.round(left), Math.round(top)];
  }));
  expect(new Set(linePositions.map(([, top]) => top)).size).toBe(2);
  expect(new Set(linePositions.map(([left]) => left)).size).toBe(5);
  for (let row = 0; row < 2; row += 1) {
    expect(new Set(linePositions.slice(row * 5, row * 5 + 5).map(([, top]) => top)).size).toBe(1);
  }

  await page.setViewportSize({ width: 390, height: 844 });
  const narrowPositions = await buttons.evaluateAll((items) => items.map((button) => {
    const { left, top } = button.getBoundingClientRect();
    return [Math.round(left), Math.round(top)];
  }));
  expect(new Set(narrowPositions.map(([, top]) => top)).size).toBe(4);
  expect(new Set(narrowPositions.map(([left]) => left)).size).toBe(5);
});

test("fills and resizes with the available browser viewport", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  const measureLayout = () => page.evaluate(() => {
    const main = document.querySelector("main").getBoundingClientRect();
    const surface = document.querySelector("#display-surface").getBoundingClientRect();
    const canvas = document
      .querySelector("#display-surface > canvas:not([hidden])")
      .getBoundingClientRect();
    return {
      viewportWidth: document.documentElement.clientWidth,
      viewportHeight: window.innerHeight,
      documentHeight: document.documentElement.scrollHeight,
      main: { width: main.width, height: main.height },
      surface: { width: surface.width, height: surface.height },
      canvas: { width: canvas.width, height: canvas.height },
    };
  });

  const desktop = await measureLayout();
  expect(desktop.main.width).toBeCloseTo(desktop.viewportWidth, 0);
  expect(desktop.main.height).toBeCloseTo(desktop.viewportHeight, 0);
  expect(desktop.documentHeight).toBeLessThanOrEqual(desktop.viewportHeight + 1);
  expect(desktop.canvas.width).toBeLessThanOrEqual(desktop.surface.width);
  expect(desktop.canvas.height).toBeLessThanOrEqual(desktop.surface.height);
  expect(desktop.canvas.width / desktop.canvas.height).toBeCloseTo(1046 / 830, 2);

  await page.setViewportSize({ width: 800, height: 600 });
  const resized = await measureLayout();
  expect(resized.main.width).toBeCloseTo(resized.viewportWidth, 0);
  expect(resized.main.height).toBeCloseTo(resized.viewportHeight, 0);
  expect(resized.documentHeight).toBeLessThanOrEqual(resized.viewportHeight + 1);
  expect(resized.canvas.width).toBeLessThan(desktop.canvas.width);
  expect(resized.canvas.width / resized.canvas.height).toBeCloseTo(1046 / 830, 2);
});

test("plots newer workload samples at the top and separates long gaps", () => {
  const old = { elapsedSeconds: 7, workloadPercent: 50 };
  const current = { elapsedSeconds: 10, workloadPercent: 125 };
  const oldPoint = workloadPointCoordinates(old, 10, 3, 200);
  const currentPoint = workloadPointCoordinates(current, 10, 3, 200);
  expect(oldPoint).toEqual({ x: 0.25, y: 1 });
  expect(currentPoint).toEqual({ x: 0.625, y: 0 });

  const segments = workloadSampleSegments([
    { elapsedSeconds: 1, workloadPercent: 10 },
    { elapsedSeconds: 1.04, workloadPercent: 20 },
    { elapsedSeconds: 1.1, workloadPercent: 30 },
  ]);
  expect(segments.map((segment) => segment.length)).toEqual([2, 1]);
});

test("retains three seconds of workload history at 120 Hz", () => {
  const samples = [];
  for (let frame = 0; frame <= 420; frame += 1) {
    retainWorkloadSample(samples, {
      elapsedSeconds: frame / 120,
      workloadPercent: 25,
    });
  }

  expect(samples.length).toBeGreaterThan(300);
  expect(samples.at(-1).elapsedSeconds - samples[0].elapsedSeconds).toBeCloseTo(3, 5);
});

test("shows live workload measurements in the Workload tools tab", async ({ page }) => {
  await page.goto(shellPath);
  const start = page.locator("#run-pause");
  await expect(start).toBeEnabled();
  await start.click();
  await expect(start).toHaveText("Pause");
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 5_000 },
  );
  await page.locator("#tools-panel > summary").click();
  await page.locator("#workload-tab").click();

  const panel = page.locator("#workload-panel");
  const summary = page.locator("#workload-summary");
  const graph = page.locator("#workload-graph");
  await expect(panel).toBeVisible();
  const toolBounds = await page.locator("#tools-panel").boundingBox();
  const graphBounds = await graph.boundingBox();
  expect(toolBounds).not.toBeNull();
  expect(graphBounds).not.toBeNull();
  expect(graphBounds.height).toBeGreaterThan(toolBounds.height * 0.5);
  await expect(summary).toContainText("x1 | average");
  await expect(summary).toContainText("peak");
  await expect(summary).toContainText("100% = one 10 ms budget");
  await expect(graph).toHaveAttribute("role", "img");

  await expect.poll(() => graph.evaluate((canvas) => {
    const pixels = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    let bluePixels = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index + 2] - pixels[index] > 40) {
        bluePixels += 1;
      }
    }
    return bluePixels;
  })).toBeGreaterThan(0);

  await page.setViewportSize({ width: 390, height: 844 });
  const narrowToolBounds = await page.locator("#tools-panel").boundingBox();
  const narrowGraphBounds = await graph.boundingBox();
  expect(narrowToolBounds).not.toBeNull();
  expect(narrowGraphBounds).not.toBeNull();
  expect(narrowGraphBounds.height).toBeGreaterThan(narrowToolBounds.height * 0.5);
});
