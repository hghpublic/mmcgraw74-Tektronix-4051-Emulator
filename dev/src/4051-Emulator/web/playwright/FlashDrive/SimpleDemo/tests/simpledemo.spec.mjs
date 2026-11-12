import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import {
  answerProgram,
  attachScreen,
  countLitPixels,
  expectTapeOperation,
  runProgram,
  useTapePrograms,
  waitForIdle,
  waitForProgramInput,
} from "../../../support/emulator.mjs";

const directory = fileURLToPath(new URL("../../../../FlashDrive/SimpleDemo/", import.meta.url));
const expectedFileNumbers = ["1", "2", "3", "9", "23", "119", "124"];
useTapePrograms(directory, expectedFileNumbers);

test("imports every SimpleDemo file with its FlashDrive number", async ({ page }) => {
  const fileNumbers = await page.locator("#fileList option").allTextContents();
  expect(fileNumbers).toEqual(expectedFileNumbers);
});

test("Simple Demo reads both picture files and chains to Instant Art", async ({ page }) => {
  await runProgram(page, 1);
  await expectTapeOperation(page, "gpibRead", 23);
  await waitForProgramInput(page);
  await expectCanvasActivity(page);
  await attachScreen(page, "Mickey Mouse");

  await answerProgram(page, "N");
  await expectTapeOperation(page, "gpibRead", 9);
  await waitForProgramInput(page);
  await attachScreen(page, "Death Star");

  await answerProgram(page, "N");
  await expectTapeOperation(page, "gpibOld", 2);
  await waitForProgramInput(page);
  await answerProgram(page, "COPILOT");
  await waitForProgramInput(page);
  await expectArtwork(page);
});

test("Instant Art draws with fixed scripted text", async ({ page }) => {
  await runProgram(page, 2);
  await waitForProgramInput(page);
  await answerProgram(page, "TEK4051");
  await waitForProgramInput(page);
  await expectArtwork(page);
});

test("Simple create runs with its imported data file", async ({ page }) => {
  const originalContent = await page.evaluate(
    () => Array.from(new Uint8Array(storage.fstore.getFile(9).content)),
  );
  await runProgram(page, 3);
  await expectTapeOperation(page, "gpibRead", 9);
  await waitForIdle(page);
  const rewrittenContent = await page.evaluate(
    () => Array.from(new Uint8Array(storage.fstore.getFile(9).content)),
  );
  expect(rewrittenContent.length).toBeGreaterThan(0);
  expect(rewrittenContent).not.toEqual(originalContent);
  await expectCanvasActivity(page);
});

test("TLIST lists the imported directory and accepts RETURN", async ({ page }) => {
  await runProgram(page, 119);
  await expectTapeOperation(page, "gpibList", 124);
  await waitForProgramInput(page);
  await expectCanvasActivity(page);
  await attachScreen(page, "Directory listing");

  await answerProgram(page, "");
  await expectTapeOperation(page, "gpibOld", 1);
  await expectTapeOperation(page, "gpibRead", 23);
  await waitForProgramInput(page);
});

async function expectArtwork(page) {
  await expect
    .poll(() => countLitPixels(page, 200), {
      message: "expected generated artwork below the text prompt",
    })
    .toBeGreaterThan(500);
}

async function expectCanvasActivity(page) {
  await expect
    .poll(() => countLitPixels(page), {
      message: "expected visible emulator output on the screen canvas",
      timeout: 90_000,
      intervals: [250, 500, 1_000],
    })
    .toBeGreaterThan(500);
}
