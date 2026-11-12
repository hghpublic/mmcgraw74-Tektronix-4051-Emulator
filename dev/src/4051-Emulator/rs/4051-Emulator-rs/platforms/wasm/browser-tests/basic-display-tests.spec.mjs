import { expect, test } from "@playwright/test";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";

async function countLitPixels(canvas) {
  return canvas.evaluate((surface) => {
    const context = surface.getContext("2d");
    if (!context) {
      throw new Error("The CPU display canvas has no 2D context");
    }
    const pixels = context.getImageData(0, 0, surface.width, surface.height).data;
    let count = 0;
    for (let index = 1; index < pixels.length; index += 4) {
      if (pixels[index] > 0) {
        count += 1;
      }
    }
    return count;
  });
}

async function countBlueDrawPixels(canvas) {
  return canvas.evaluate((surface) => {
    const context = surface.getContext("2d");
    if (!context) {
      throw new Error("The CPU display canvas has no 2D context");
    }
    const pixels = context.getImageData(0, 0, surface.width, surface.height).data;
    let count = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index + 3] > 0 && pixels[index + 2] > 0) {
        count += 1;
      }
    }
    return count;
  });
}

async function findMissingLitPoints(canvas, expectedPoints) {
  return canvas.evaluate((surface, points) => {
    const context = surface.getContext("2d");
    if (!context) {
      throw new Error("The CPU display canvas has no 2D context");
    }
    const pixels = context.getImageData(0, 0, surface.width, surface.height).data;
    return points.filter(([x, y]) => {
      const centerX = Math.round(x + 16);
      const centerY = surface.height - 1 - Math.round(y + 10);
      for (let yOffset = -1; yOffset <= 1; yOffset += 1) {
        for (let xOffset = -1; xOffset <= 1; xOffset += 1) {
          const pixelX = centerX + xOffset;
          const pixelY = centerY + yOffset;
          if (
            pixelX >= 0
            && pixelX < surface.width
            && pixelY >= 0
            && pixelY < surface.height
            && pixels[(pixelY * surface.width + pixelX) * 4 + 1] > 0
          ) {
            return false;
          }
        }
      }
      return true;
    });
  }, expectedPoints);
}

async function findStrongGridLines(canvas) {
  return canvas.evaluate((surface) => {
    const sourceContext = surface.getContext("2d");
    if (!sourceContext) {
      throw new Error("The CPU display canvas has no 2D context");
    }
    const bounds = surface.getBoundingClientRect();
    const presented = document.createElement("canvas");
    presented.width = Math.round(bounds.width);
    presented.height = Math.round(bounds.height);
    const context = presented.getContext("2d");
    if (!context) {
      throw new Error("The scaled display canvas has no 2D context");
    }
    const imageRendering = getComputedStyle(surface).imageRendering;
    if (imageRendering !== "auto") {
      throw new Error(`Expected filtered canvas scaling, received image-rendering: ${imageRendering}`);
    }
    context.imageSmoothingEnabled = true;
    context.drawImage(surface, 0, 0, presented.width, presented.height);
    const pixels = context.getImageData(0, 0, presented.width, presented.height).data;
    const columns = Array(presented.width).fill(0);
    const rows = Array(presented.height).fill(0);
    for (let y = 0; y < presented.height; y += 1) {
      for (let x = 0; x < presented.width; x += 1) {
        if (pixels[(y * presented.width + x) * 4 + 1] > 0) {
          columns[x] += 1;
          rows[y] += 1;
        }
      }
    }

    function findLineCenters(counts) {
      const centers = [];
      let start = null;
      let end = null;
      for (let index = 0; index <= counts.length; index += 1) {
        if (index < counts.length && counts[index] > 200) {
          start ??= index;
          end = index;
        } else if (start !== null && end !== null) {
          centers.push(Math.round((start + end) / 2));
          start = null;
          end = null;
        }
      }
      return centers;
    }

    return {
      vertical: findLineCenters(columns),
      horizontal: findLineCenters(rows),
      width: presented.width,
      height: presented.height,
    };
  });
}

test("runs the grid, snake-fill, and single-circle programs through BASIC", async ({ page }, testInfo) => {
  test.setTimeout(300_000);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      value: undefined,
    });
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (type, options) {
      if (this.id === "screen" && type === "2d") {
        this.style.backgroundColor = "blue";
        return getContext.call(this, type, { ...options, alpha: true });
      }
      return getContext.call(this, type, options);
    };
    const putImageData = CanvasRenderingContext2D.prototype.putImageData;
    CanvasRenderingContext2D.prototype.putImageData = function (imageData, ...args) {
      if (this.canvas.id === "screen") {
        const pixels = imageData.data;
        for (let index = 0; index < pixels.length; index += 4) {
          if (pixels[index] === 0 && pixels[index + 1] === 0 && pixels[index + 2] === 0) {
            pixels[index + 3] = 0;
          }
        }
      }
      return putImageData.call(this, imageData, ...args);
    };
  });
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();

  await page.locator("#tools-panel > summary").click();
  await page.locator("#test-tab").click();
  await expect(page.locator("#test-panel")).toBeVisible();
  const display = page.locator("#screen");
  await expect(display).toBeVisible();

  for (const [id, label] of [
    ["single-circle", "Single circle"],
    ["grid", "Grid"],
    ["snake", "Snake fill"],
  ]) {
    const button = page.locator(`[data-basic-test="${id}"]`);
    await expect(button).toBeEnabled();
    await button.click();
    const status = page.locator("#basic-test-status");
    await expect(status).toHaveText(
      new RegExp(`^${label} test (?:complete\\.|failed:)`),
      { timeout: 240_000 },
    );
    if (id === "single-circle") {
      await display.screenshot({
        path: testInfo.outputPath("single-circle-diagnostic.png"),
      });
    }
    await expect(status).toHaveText(
      `${label} test complete.`,
    );
    const litPixels = await countLitPixels(display);
    const minimumLitPixels = id === "snake" ? 10_000 : id === "single-circle" ? 1_500 : 5_000;
    expect(litPixels, `${label} should draw visible geometry`)
      .toBeGreaterThanOrEqual(minimumLitPixels);
    if (id === "single-circle") {
      const expectedPoints = [
        [872, 390],
        [152, 390],
        [512, 750],
        [512, 30],
      ];
      expect(
        await findMissingLitPoints(display, expectedPoints),
        "The circle should include its four cardinal points",
      ).toEqual([]);
    }
    expect(await countBlueDrawPixels(display), `${label} should not draw blue geometry`).toBe(0);
    const background = await display.evaluate((surface) => {
      const context = surface.getContext("2d");
      if (!context) {
        throw new Error("The CPU display canvas has no 2D context");
      }
      return {
        color: getComputedStyle(surface).backgroundColor,
        pixel: [...context.getImageData(0, 0, 1, 1).data],
      };
    });
    expect(background, `${label} should clear to a blue background behind its drawing`)
      .toEqual({ color: "rgb(0, 0, 255)", pixel: [0, 0, 0, 0] });
    if (id === "grid") {
      const observedLines = await findStrongGridLines(display);
      await display.screenshot({
        path: testInfo.outputPath("grid-diagnostic.png"),
      });
      expect(
        observedLines.vertical,
        `The grid should show two borders and 8 inner vertical lines: ${observedLines.vertical}`,
      )
        .toHaveLength(10);
      expect(
        observedLines.horizontal,
        `The grid should show two borders and 8 inner horizontal lines: ${observedLines.horizontal}`,
      )
        .toHaveLength(10);
      expect(observedLines.vertical[0], "The left border should be near the display edge")
        .toBeLessThan(observedLines.width * 0.07);
      expect(
        observedLines.vertical[observedLines.vertical.length - 1],
        "The right border should be near the display edge",
      ).toBeGreaterThan(observedLines.width * 0.93);
      expect(observedLines.horizontal[0], "The top border should be near the display edge")
        .toBeLessThan(observedLines.height * 0.07);
      expect(
        observedLines.horizontal[observedLines.horizontal.length - 1],
        "The bottom border should be near the display edge",
      ).toBeGreaterThan(observedLines.height * 0.93);
      for (const [orientation, positions] of Object.entries({
        vertical: observedLines.vertical,
        horizontal: observedLines.horizontal,
      })) {
        const gaps = positions.slice(1).map((position, index) => position - positions[index]);
        const largestGap = Math.max(...gaps);
        const smallestGap = Math.min(...gaps);
        expect(
          largestGap - smallestGap,
          `${orientation} grid lines should be evenly spaced`,
        ).toBeLessThanOrEqual(2);
      }
    }
    await expect(page.locator("#basic-input-status")).toHaveAttribute(
      "aria-label",
      "BASIC prompt detected",
    );
  }
});
