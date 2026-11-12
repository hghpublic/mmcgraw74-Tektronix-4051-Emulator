import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  answerProgram,
  attachScreen,
  countLitPixels,
  expectTapeOperation,
  runProgram,
  setCpuSpeed,
  useTapePrograms,
  waitForIdle,
  waitForProgramInput,
} from "../../../support/emulator.mjs";

const directory = fileURLToPath(new URL("../../../../FlashDrive/Battlestar/", import.meta.url));
const expectedFileNumbers = [
  ...Array.from({ length: 90 }, (_, index) => index + 1).filter((number) => number !== 55),
  119, 124,
].map(String);
useTapePrograms(directory, expectedFileNumbers);

test.beforeEach(async ({ page }) => {
  await page.evaluate(() => {
    window.battlestarWrites = {};
    const original = storage.gpibWrite;
    storage.gpibWrite = function (...args) {
      const result = original.apply(this, args);
      const number = this.currentTekFile.num;
      window.battlestarWrites[number] = (window.battlestarWrites[number] ?? 0) + 1;
      return result;
    };
  });
});

test("imports all 91 Battlestar assets and preserves the large binary picture", async ({ page }) => {
  expect(await page.locator("#fileList option").allTextContents()).toEqual(expectedFileNumbers);
  const original = createHash("sha256").update(readFileSync(
    `${directory}/15     BINARY  DATA BattlestarGalac3 332234`,
  )).digest("hex");
  const imported = await page.evaluate(async () => {
    const digest = await crypto.subtle.digest("SHA-256", storage.fstore.getFile(15).content);
    return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  });
  expect(imported).toBe(original);
});

test.afterEach(async ({ page }, testInfo) => {
  const path = testInfo.outputPath("picture-writes.json");
  await writeFile(path, JSON.stringify(await page.evaluate(() => window.battlestarWrites), null, 2));
  await testInfo.attach("picture-writes", { path, contentType: "application/json" });
});

test.skip("original demo (file 1) cannot load: Option 30 ROPEN syntax at BASIC line 3490", async () => {});

for (const { program, output, label, title, minimumBytes } of [
  { program: 2, output: 12, label: "Colonial Vipers on Patrol", title: "Vipers on patrol", minimumBytes: 500 },
  { program: 26, output: 27, title: "Cylon tanker text", minimumBytes: 100 },
  { program: 29, output: 30, label: "Targeting Circles", title: "Targeting circles", minimumBytes: 500 },
]) {
  test(`${title} creates a complete binary picture in browser memory`, async ({ page }) => {
    await runProgram(page, program);
    await waitForIdle(page);
    const result = await page.evaluate((number) => {
      const content = storage.fstore.getFile(number).content;
      return {
        bytes: content.byteLength,
        text: new TextDecoder().decode(content),
        written: window.battlestarWrites[number] ?? 0,
      };
    }, output);
    expect(result.written).toBeGreaterThan(minimumBytes);
    expect(result.bytes).toBe(result.written);
    if (label !== undefined) {
      expect(result.text).toContain(label);
    }
    expect(result.text).toContain("|EOG|");
    expect(await countLitPixels(page)).toBeGreaterThan(500);
    await attachScreen(page, title);
  });
}

test("Cylon conversion writes a new R12 object to file 9", async ({ page }) => {
  await runProgram(page, 8);
  await expectTapeOperation(page, "gpibRead", 7);
  await waitForIdle(page);
  const result = await page.evaluate(() => ({
    bytes: storage.fstore.getFile(9).content.byteLength,
    written: window.battlestarWrites[9] ?? 0,
  }));
  expect(result.written).toBeGreaterThan(400);
  expect(result.bytes).toBe(result.written);
  await attachScreen(page, "Converted Cylon object");
});

test("Cylon Attack starts with UDKs and UDK10 attempts to reload the blocked demo", async ({ page }, testInfo) => {
  testInfo.annotations.push({
    type: "known-limitation",
    description: "UDK10 loads original file 1, which cannot parse Option 30 ROPEN; no restored menu is claimed.",
  });
  await runProgram(page, 4);
  await answerProgram(page, "3");
  await pressTekKey(page, 0x67);
  await expectTapeOperation(page, "gpibRead", 5);
  await test.step("Waiting for targeting graphics after the PAGE erase flash", async () => {
    await page.waitForFunction(() => {
      const canvas = document.getElementById("screen");
      const pixels = canvas.getContext("2d").getImageData(0, 300, canvas.width, canvas.height - 300).data;
      let lit = 0;
      for (let index = 0; index < pixels.length; index += 4) {
        if (pixels[index + 1] > 0) lit += 1;
      }
      return lit > 2_000 && lit < canvas.width * (canvas.height - 300) / 5
        && getComputedStyle(document.getElementById("status_busy")).color === "rgb(0, 238, 0)";
    });
  });
  await attachScreen(page, "Cylon Attack UDK mode");
  await pressTekKey(page, 0x69);
  await expectTapeOperation(page, "gpibOld", 1);
  await waitForIdle(page);
  await attachScreen(page, "Known original-demo load limitation");
});

test("TLIST pages through the directory and RETURN attempts the blocked demo load", async ({ page }, testInfo) => {
  testInfo.annotations.push({
    type: "known-limitation",
    description: "RETURN loads original file 1, whose Option 30 syntax is unsupported on the 4051.",
  });
  await runProgram(page, 119);
  await page.evaluate(() => {
    window.battlestarListingProgress = { count: 0, changed: performance.now() };
  });
  for (let screen = 1; screen <= 5; screen += 1) {
    await test.step(`Waiting for directory screen ${screen} or the LAST marker`, async () => {
      await page.waitForFunction(() => {
        const events = window.tapeEvents.filter((event) => event.operation === "gpibList");
        if (events.some((event) => event.fileNumber === 124)) return true;
        const progress = window.battlestarListingProgress;
        if (events.length !== progress.count) {
          progress.count = events.length;
          progress.changed = performance.now();
        }
        return events.length > 0 && performance.now() - progress.changed >= 500;
      }, null, { timeout: 30_000 });
    });
    await attachScreen(page, `Battlestar directory screen ${screen}`);
    const finished = await page.evaluate(
      () => window.tapeEvents.some((event) => event.operation === "gpibList" && event.fileNumber === 124),
    );
    if (finished) break;
    await pressTekKey(page, 0x6A);
    await page.evaluate(() => {
      window.battlestarListingProgress.changed = performance.now();
    });
  }
  await expectTapeOperation(page, "gpibList", 124);
  await waitForProgramInput(page);
  expect(await page.evaluate(
    () => window.tapeEvents.filter((event) => event.operation === "gpibList").map((event) => event.fileNumber),
  )).toEqual(Array.from({ length: 124 }, (_, index) => index + 1));
  await answerProgram(page, "");
  await expectTapeOperation(page, "gpibOld", 1);
  await waitForIdle(page);
  await attachScreen(page, "TLIST return hits original-demo load limitation");
});

test.skip("Option 30 game (file 6) requires 4054 refresh-graphics statements", async () => {});
test.skip("MUSIC utility (file 28) requires the unsupported MUSIC ROM call", async () => {});

async function pressTekKey(page, scancode) {
  await setCpuSpeed(page, 1);
  await page.evaluate(async (code) => {
    await tek.fcnkey_action(code);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }, scancode);
  await setCpuSpeed(page, 100);
}
