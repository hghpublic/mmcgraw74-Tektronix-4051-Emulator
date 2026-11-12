import { expect, test } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  attachScreen,
  countLitPixels,
  expectTapeOperation,
  runProgram,
  sendTekText,
  setCpuSpeed,
  useTapePrograms,
  waitForIdle,
  waitForProgramInput,
} from "../../support/emulator.mjs";

const directory = fileURLToPath(new URL("../../../Benchmark/", import.meta.url));
const expectedFileNumbers = ["1", "2", "3", "4", "10", "11", "12"];
const speeds = [1, 2, 5, 10, 20, 50, 100];
useTapePrograms(directory, expectedFileNumbers);

test("imports the benchmark programs, reference results, and LAST marker", async ({ page }) => {
  expect(await page.locator("#fileList option").allTextContents()).toEqual(expectedFileNumbers);
  const reference = await readResults(page);
  expect(reference.match(/Emulator Speed:/g)).toHaveLength(7);
  expect(reference.trimEnd()).toMatch(/\|EOT\|$/);
});

test("Benchmark menu selects the emulator benchmark with uppercase Y", async ({ page }) => {
  await runProgram(page, 1);
  await answerMenu(page, "Y");
  await expectTapeOperation(page, "gpibOld", 2);
  await expectTapeOperation(page, "gpibFind", 10);
  await waitForProgramInput(page);
  await attachScreen(page, "Emulator benchmark prompt");
});

test("Benchmark menu selects and completes the 4050 calculation benchmark with N", async ({ page }) => {
  await runProgram(page, 1);
  await answerMenu(page, "N");
  await expectTapeOperation(page, "gpibOld", 3);
  await waitForProgramInput(page);
  const promptPixels = await countLitPixels(page);
  await sendTekText(page, "\r");
  await waitForIdle(page);
  expect(await countLitPixels(page)).toBeGreaterThan(promptPixels);
  await attachScreen(page, "4050 benchmark completed");
});

test("emulator speed sweep records seven timings and exposes the file-51 readback limitation", async ({ page }, testInfo) => {
  testInfo.annotations.push({
    type: "known-limitation",
    description: "The unchanged program writes file 10 but attempts final readback from missing file 51.",
  });
  await runProgram(page, 2);
  for (let index = 0; index < speeds.length; index += 1) {
    await waitForProgramInput(page);
    await setCpuSpeed(page, speeds[index]);
    await sendTekText(page, "\r");
    await test.step(`Waiting for result at x${speeds[index]}`, async () => {
      await page.waitForFunction(
        (count) => {
          const text = new TextDecoder().decode(storage.fstore.getFile(10).content);
          return (text.match(/Emulator Speed:/g) ?? []).length === count;
        },
        index + 1,
        { timeout: 30_000 },
      );
    });
    await attachScreen(page, `Emulator speed x${speeds[index]}`);
  }
  await expectTapeOperation(page, "gpibInput", 51);
  await test.step("Confirming the unchanged program loops reading missing file 51", async () => {
    await page.waitForFunction(
      () => window.tapeEvents.filter(
        (event) => event.operation === "gpibInput" && event.fileNumber === 51,
      ).length >= 2,
      null,
      { timeout: 30_000 },
    );
    await expect(page.locator("#status_busy")).toHaveCSS("color", "rgb(0, 238, 0)");
  });
  const results = await readResults(page);
  const rows = results.split("\r").filter((line) => line.includes("Emulator Speed:"));
  expect(rows).toHaveLength(7);
  for (let index = 0; index < speeds.length; index += 1) {
    expect(rows[index]).toMatch(new RegExp(
      `^\\s*\\d+\\.\\d{2} second\\(s\\), at Emulator Speed:\\s+${speeds[index]}X$`,
    ));
  }
  expect(results.trimEnd()).toMatch(/\|EOT\|$/);
  const path = testInfo.outputPath("benchmark-results.txt");
  await writeFile(path, results);
  await testInfo.attach("measured-results", { path, contentType: "text/plain" });
  await attachScreen(page, "Known file-51 readback limitation");
});

test("TLIST lists the benchmark directory and RETURN reloads its menu", async ({ page }) => {
  await runProgram(page, 11);
  await expectTapeOperation(page, "gpibList", 12);
  await waitForProgramInput(page);
  await attachScreen(page, "Benchmark directory listing");
  await setCpuSpeed(page, 1);
  await sendTekText(page, "\r");
  await setCpuSpeed(page, 100);
  await expectTapeOperation(page, "gpibOld", 1);
  await waitForProgramInput(page);
  await attachScreen(page, "Benchmark menu restored");
});

test.skip("MFM benchmark needs the unavailable MFM !TIME clock ROM (program 4)", async () => {});

async function answerMenu(page, answer) {
  await waitForProgramInput(page);
  await setCpuSpeed(page, 1);
  await page.keyboard.down("Shift");
  try {
    await sendTekText(page, answer);
  } finally {
    await page.keyboard.up("Shift");
  }
  await sendTekText(page, "\r");
  await setCpuSpeed(page, 100);
}

async function readResults(page) {
  return page.evaluate(() => new TextDecoder().decode(storage.fstore.getFile(10).content));
}
