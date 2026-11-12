import { readFile } from "node:fs/promises";
import { expect, test } from "@playwright/test";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";
const scriptDirectory = new URL("./test-scripts/", import.meta.url);
const geometryScripts = [
  {
    fileNumber: 21,
    fileName: "21     ASCII   PROG [H-LINES] 680.bas",
    description: "H-LINES",
    label: "20 parallel horizontal lines",
    expectedSegmentCount: 20,
    minimumLitPixels: 15_000,
  },
  {
    fileNumber: 22,
    fileName: "22     ASCII   PROG [V-LINES] 684.bas",
    description: "V-LINES",
    label: "20 parallel vertical lines",
    expectedSegmentCount: 20,
    minimumLitPixels: 11_000,
  },
  {
    fileNumber: 23,
    fileName: "23     ASCII   PROG [ANG-LINES] 683.bas",
    description: "ANG-LINES",
    label: "20 parallel angled lines",
    expectedSegmentCount: 20,
    minimumLitPixels: 14_000,
  },
  {
    fileNumber: 24,
    fileName: "24     ASCII   PROG [FIX-RAND] 697.bas",
    description: "FIX-RAND",
    label: "20 fixed random lines",
    expectedSegmentCount: 20,
    minimumLitPixels: 6_000,
  },
  {
    fileNumber: 25,
    fileName: "25     ASCII   PROG [NEST-TRI] 1382.bas",
    description: "NEST-TRI",
    label: "20 nested centered triangles",
    expectedSegmentCount: 60,
    nestedShape: "triangle",
    minimumLitPixels: 16_000,
  },
  {
    fileNumber: 26,
    fileName: "26     ASCII   PROG [NEST-RECT] 1738.bas",
    description: "NEST-RECT",
    label: "20 nested centered rectangles",
    expectedSegmentCount: 80,
    nestedShape: "rectangle",
    minimumLitPixels: 14_000,
  },
  {
    fileNumber: 27,
    fileName: "27     ASCII   PROG [ROT-RECT] 1728.bas",
    description: "ROT-RECT",
    label: "20 nested centered rectangles rotated 45 degrees",
    expectedSegmentCount: 80,
    nestedShape: "rotated-rectangle",
    minimumLitPixels: 14_000,
  },
  {
    fileNumber: 28,
    fileName: "28     ASCII   PROG [CIRCLE] 832.bas",
    description: "CIRCLE",
    label: "single 24-segment circle via Flash Drive",
    expectedSegmentCount: 24,
    geometry: "circle",
    minimumLitPixels: 1_500,
  },
];

async function openFlashDrive(page) {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      value: undefined,
    });
  });
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  await page.locator("#tools-panel > summary").click();
  await page.locator("#flash-drive-tab").click();
}

async function readFlashDriveFixtures() {
  return Promise.all(geometryScripts.map(async (script) => {
    const source = await readFile(new URL(script.fileName, scriptDirectory), "utf8");
    const buffer = Buffer.from(source);
    return {
      ...script,
      source,
      byteLength: buffer.length,
      file: {
        name: script.fileName,
        mimeType: "application/octet-stream",
        buffer,
      },
    };
  }));
}

function expectedCircleSegments() {
  const points = Array.from({ length: 24 }, (_, index) => {
    const angle = 2 * Math.PI * index / 24;
    return [
      Math.round(512 + 360 * Math.cos(angle)),
      Math.round(390 + 360 * Math.sin(angle)),
    ];
  });
  return points.map((point, index) => [
    point,
    points[(index + 1) % points.length],
  ]);
}

function polygonSegments(vertices) {
  return vertices.map((point, index) => [
    point,
    vertices[(index + 1) % vertices.length],
  ]);
}

function expectedNestedSegments(shape) {
  const centerX = 512;
  const centerY = 384;
  return Array.from({ length: 20 }, (_, index) => {
    const size = index + 1;
    switch (shape) {
      case "triangle":
        return polygonSegments([
          [centerX, centerY + 15 * size],
          [centerX - 19 * size, centerY - 15 * size],
          [centerX + 19 * size, centerY - 15 * size],
        ]);
      case "rectangle":
        return polygonSegments([
          [centerX - 16 * size, centerY - 9 * size],
          [centerX + 16 * size, centerY - 9 * size],
          [centerX + 16 * size, centerY + 9 * size],
          [centerX - 16 * size, centerY + 9 * size],
        ]);
      case "rotated-rectangle":
        return polygonSegments([
          [centerX - 5 * size, centerY - 18 * size],
          [centerX + 18 * size, centerY - 5 * size],
          [centerX + 5 * size, centerY + 18 * size],
          [centerX - 18 * size, centerY + 5 * size],
        ]);
      default:
        throw new Error(`Unknown nested geometry shape: ${shape}`);
    }
  }).flat();
}

function readSegments(script) {
  const segments = [];
  let current = null;
  for (const line of script.split(/\r\n|\n|\r/u)) {
    const match = line.match(/^\s*\d+\s+(MOVE|DRAW)\s+(\d+)\s*,\s*(\d+)\s*$/iu);
    if (!match) {
      continue;
    }
    const point = [Number(match[2]), Number(match[3])];
    if (match[1].toUpperCase() === "MOVE") {
      current = point;
    } else {
      if (!current) {
        throw new Error(`DRAW has no preceding MOVE in BASIC line: ${line}`);
      }
      segments.push([current, point]);
      current = point;
    }
  }
  return segments;
}

function segmentProbePoints(segments) {
  return segments.flatMap(([from, to]) => [0.25, 0.5, 0.75].map((fraction) => [
    Math.round(from[0] + (to[0] - from[0]) * fraction),
    Math.round(from[1] + (to[1] - from[1]) * fraction),
  ]));
}

async function sampleGeometry(canvas, points) {
  return canvas.evaluate((surface, expectedPoints) => {
    const sourceContext = surface.getContext("2d");
    if (!sourceContext) {
      throw new Error("The reference display canvas has no 2D context");
    }
    const sourcePixels = sourceContext.getImageData(0, 0, surface.width, surface.height).data;
    const bounds = surface.getBoundingClientRect();
    const presented = document.createElement("canvas");
    presented.width = Math.max(1, Math.round(bounds.width));
    presented.height = Math.max(1, Math.round(bounds.height));
    const presentedContext = presented.getContext("2d");
    if (!presentedContext) {
      throw new Error("The scaled display canvas has no 2D context");
    }
    presentedContext.imageSmoothingEnabled = true;
    presentedContext.drawImage(surface, 0, 0, presented.width, presented.height);
    const presentedPixels = presentedContext.getImageData(
      0,
      0,
      presented.width,
      presented.height,
    ).data;

    function isGreenNear(pixels, width, height, x, y, radius) {
      for (let yOffset = -radius; yOffset <= radius; yOffset += 1) {
        for (let xOffset = -radius; xOffset <= radius; xOffset += 1) {
          const pixelX = x + xOffset;
          const pixelY = y + yOffset;
          if (
            pixelX >= 0
            && pixelX < width
            && pixelY >= 0
            && pixelY < height
            && pixels[(pixelY * width + pixelX) * 4 + 1] > 0
          ) {
            return true;
          }
        }
      }
      return false;
    }

    const missingFramebufferPoints = [];
    const missingPresentedPoints = [];
    for (const [x, y] of expectedPoints) {
      const framebufferX = Math.round(x + 16);
      const framebufferY = surface.height - 1 - Math.round(y + 10);
      if (!isGreenNear(
        sourcePixels,
        surface.width,
        surface.height,
        framebufferX,
        framebufferY,
        1,
      )) {
        missingFramebufferPoints.push([x, y]);
      }
      const presentedX = Math.round(framebufferX * presented.width / surface.width);
      const presentedY = Math.round(framebufferY * presented.height / surface.height);
      if (!isGreenNear(
        presentedPixels,
        presented.width,
        presented.height,
        presentedX,
        presentedY,
        2,
      )) {
        missingPresentedPoints.push([x, y]);
      }
    }
    return { missingFramebufferPoints, missingPresentedPoints };
  }, points);
}

test("imports BASIC fixtures with valid Flash Drive filenames", async ({ page }) => {
  await openFlashDrive(page);
  const fixtures = await readFlashDriveFixtures();
  await page.locator("#import-files-picker").setInputFiles(
    fixtures.map(({ file }) => file),
  );
  await expect(page.locator("#flash-status")).toContainText(
    `Processed ${fixtures.length} selected file(s)`,
    { timeout: 30_000 },
  );
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(fixtures.length);

  for (const fixture of fixtures) {
    expect(fixture.source, `${fixture.fileName} should use BASIC carriage-return line endings`)
      .toContain("\r");
    expect(fixture.source, `${fixture.fileName} should not contain line feeds`)
      .not.toContain("\n");
    const row = page.locator(
      `#flash-directory-body tr[data-file-number="${fixture.fileNumber}"]`,
    );
    await expect(row.locator("td")).toHaveText([
      String(fixture.fileNumber),
      "ASCII",
      "PROG",
      String(fixture.byteLength),
      fixture.description,
    ]);
  }
});

test("runs geometry BASIC scripts at framebuffer and downscaled presentation sizes", async ({ page }, testInfo) => {
  test.setTimeout(300_000);
  await openFlashDrive(page);
  const fixtures = await readFlashDriveFixtures();
  await page.locator("#import-files-picker").setInputFiles(
    fixtures.map(({ file }) => file),
  );
  await expect(page.locator("#flash-status")).toContainText(
    `Processed ${fixtures.length} selected file(s)`,
    { timeout: 30_000 },
  );
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(fixtures.length);

  await page.locator("#run-pause").click();
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 10_000 },
  );
  const display = page.locator("#screen");

  for (const [index, script] of fixtures.entries()) {
    await test.step(script.label, async () => {
      if (index > 0) {
        await page.locator("#speed").selectOption("0");
        await page.locator("#reset").click();
        await expect(page.locator("#basic-prompt-status")).toContainText(
          "BASIC prompt detected",
          { timeout: 10_000 },
        );
      }

      const segments = readSegments(script.source);
      expect(segments, `${script.label} should contain its specified MOVE/DRAW geometry`)
        .toHaveLength(script.expectedSegmentCount);
      if (script.nestedShape) {
        expect(segments, `${script.label} should be centered and increase evenly in size`)
          .toEqual(expectedNestedSegments(script.nestedShape));
      }
      if (script.geometry === "circle") {
        expect(segments, `${script.label} should match the precomputed circular vertices`)
          .toEqual(expectedCircleSegments());
      }
      const probePoints = segmentProbePoints(segments);
      const row = page.locator(
        `#flash-directory-body tr[data-file-number="${script.fileNumber}"]`,
      );
      await row.click();
      await expect(page.locator("#launch-file")).toBeEnabled();
      await page.locator("#launch-file").click();
      await expect(page.locator("#flash-status")).toContainText(
        `Program launch requested for Flash Drive file ${script.fileNumber}`,
      );

      await expect(page.locator("#launch-file")).toBeDisabled();
      await expect(page.locator("#launch-file")).toBeEnabled({ timeout: 60_000 });
      await expect(page.locator("#basic-prompt-status")).toContainText(
        "BASIC prompt detected",
        { timeout: 30_000 },
      );
      await expect.poll(
        () => sampleGeometry(display, probePoints),
        {
          message: `${script.label} should survive framebuffer mapping and CSS downscaling`,
          timeout: 30_000,
        },
      ).toEqual({
        missingFramebufferPoints: [],
        missingPresentedPoints: [],
      });
      const litPixels = await display.evaluate((surface) => {
        const context = surface.getContext("2d");
        if (!context) {
          throw new Error("The reference display canvas has no 2D context");
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
      expect(litPixels, `${script.label} should draw enough visible output`)
        .toBeGreaterThanOrEqual(script.minimumLitPixels);
      await display.screenshot({
        path: testInfo.outputPath(`${script.fileNumber}.png`),
      });
    });
  }
});
