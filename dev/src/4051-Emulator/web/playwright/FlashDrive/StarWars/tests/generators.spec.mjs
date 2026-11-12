import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import {
  attachScreen,
  countLitPixels,
  expectTapeOperation,
  runProgram,
  sendTekText,
  setCpuSpeed,
  waitForIdle,
} from "../../../support/emulator.mjs";
import { directory, expectedFileNumbers, useStarWarsTape } from "./tape.mjs";

useStarWarsTape();

test.beforeEach(async ({ page }) => {
  await page.evaluate(() => {
    window.starwarsWrites = {};
    const original = storage.gpibWrite;
    storage.gpibWrite = function (...args) {
      const result = original.apply(this, args);
      const number = this.currentTekFile.num;
      window.starwarsWrites[number] = (window.starwarsWrites[number] ?? 0) + 1;
      return result;
    };
  });
});

test.afterEach(async ({ page }, testInfo) => {
  const path = testInfo.outputPath("vector-writes.json");
  await writeFile(path, JSON.stringify(await page.evaluate(() => window.starwarsWrites), null, 2));
  await testInfo.attach("vector-writes", { path, contentType: "application/json" });
});

test("imports all 11 assets and preserves every binary vector file", async ({ page }) => {
  expect(await page.locator("#fileList option").allTextContents()).toEqual(expectedFileNumbers);
  for (const name of readdirSync(directory).filter((name) => name.includes("BINARY"))) {
    const number = Number(name.match(/^\d+/)[0]);
    const original = createHash("sha256").update(readFileSync(`${directory}/${name}`)).digest("hex");
    const imported = await page.evaluate(async (number) => {
      const digest = await crypto.subtle.digest("SHA-256", storage.fstore.getFile(number).content);
      return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
    }, number);
    expect(imported, `binary file ${number}`).toBe(original);
  }
});

for (const { program, output, label } of [
  { program: 1, output: 6, label: "Half-scale TIE fighter" },
  { program: 12, output: 5, label: "TIE fighter generator 12" },
  { program: 13, output: 5, label: "TIE fighter generator 13" },
  { program: 14, output: 5, label: "R12 TIE fighter generator" },
]) {
  test(`${label} writes its numeric vector array`, async ({ page }) => {
    await runProgram(page, program);
    await waitForIdle(page);
    const result = await page.evaluate((number) => ({
      bytes: storage.fstore.getFile(number).content.byteLength,
      written: window.starwarsWrites[number] ?? 0,
    }), output);
    expect(result.written, "133 vectors x 3 numbers x 10-byte numeric format").toBe(3990);
    expect(result.bytes).toBe(result.written);
    expect(await countLitPixels(page, 200)).toBeGreaterThan(500);
    await attachScreen(page, label);

    await setCpuSpeed(page, 1);
    await sendTekText(page, "RUN 3000\r");
    await setCpuSpeed(page, 100);
    await expectTapeOperation(page, "gpibRead", output);
    await waitForIdle(page);
    expect(await countLitPixels(page, 200)).toBeGreaterThan(500);
    await attachScreen(page, `${label} readback`);
  });
}

test.skip("file 14 RUN 5000: observed subscript error 10 at line 5080 after duplicate line 5020 replaces DIM", async () => {});
