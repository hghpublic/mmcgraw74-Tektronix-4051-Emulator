import { expect, test } from "@playwright/test";
import { readdirSync, statSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";

export function useTapePrograms(directory, expectedFileNumbers) {
  const files = readdirSync(directory)
    .map((name) => resolve(directory, name))
    .filter((path) => statSync(path).isFile());
  const pageErrors = new WeakMap();
  const unexpectedDialogs = new WeakMap();

  test.beforeEach(async ({ page }) => {
    pageErrors.set(page, []);
    unexpectedDialogs.set(page, []);
    page.on("pageerror", (error) => pageErrors.get(page).push(error.message));
    page.on("dialog", async (dialog) => {
      if (dialog.type() === "confirm"
        && dialog.message() === "WARNING: This will overwite all existing files with the same file number!") {
        await dialog.accept();
        return;
      }
      unexpectedDialogs.get(page).push(`${dialog.type()}: ${dialog.message()}`);
      await dialog.dismiss();
    });

    await page.goto("/Tektronix4051.html");
    await page.evaluate(() => {
      window.tapeEvents = [];
      for (const operation of [
        "gpibFind", "gpibOld", "gpibRead", "gpibList", "gpibPrint", "gpibInput",
      ]) {
        const original = storage[operation];
        storage[operation] = function (...args) {
          const result = original.apply(this, args);
          window.tapeEvents.push({ operation, fileNumber: this.currentTekFile.num });
          return result;
        };
      }
    });
    await page.locator("#start").click();
    await expect(page.locator("#storagedlg")).toBeEnabled();

    const uploadedNumbers = files
      .map((path) => basename(path).match(/^\s*(\d+)/)?.[1])
      .filter(Boolean)
      .sort((left, right) => Number(left) - Number(right));
    expect(uploadedNumbers).toEqual(expectedFileNumbers);
    await page.locator("#storagedlg").click();
    await page.locator("#importType").selectOption({ label: "4050 Flash Drive files" });
    const chooserPromise = page.waitForEvent("filechooser");
    await page.locator('#storageDialog input[value="Import"]').click();
    await (await chooserPromise).setFiles(files);
    await expect(page.locator("#storageMsg")).toContainText("Done.");
    expect(await page.locator("#fileList option").allTextContents()).toEqual(expectedFileNumbers);
    await page.locator('#storageDialog input[value="Cancel"]').click();

    await test.step("Waiting for ROM startup to initialize the BUSY lamp", async () => {
      await page.waitForFunction(
        () => document.getElementById("status_busy").style.color === "rgb(0, 85, 0)",
        null,
        { timeout: 30_000 },
      );
    });
  });

  test.afterEach(async ({ page }, testInfo) => {
    const path = testInfo.outputPath("tape-operations.json");
    await writeFile(path, JSON.stringify(await page.evaluate(() => window.tapeEvents), null, 2));
    await testInfo.attach("tape-operations", { path, contentType: "application/json" });
    expect(pageErrors.get(page), "uncaught browser errors").toEqual([]);
    expect(unexpectedDialogs.get(page), "unexpected browser dialogs").toEqual([]);
  });
}

export async function runProgram(page, fileNumber) {
  await setCpuSpeed(page, 1);
  await sendTekText(page, `FIND@5:${fileNumber}\r`);
  await expectTapeOperation(page, "gpibFind", fileNumber);
  await waitForIdle(page);
  await sendTekText(page, "OLD@5:\r");
  await setCpuSpeed(page, 100);
  await expectTapeOperation(page, "gpibOld", fileNumber);
  await waitForIdle(page);
  await setCpuSpeed(page, 1);
  await sendTekText(page, "RUN\r");
  await setCpuSpeed(page, 100);
}

export async function waitForIdle(page) {
  await expect(page.locator("#status_busy")).toHaveCSS("color", "rgb(0, 85, 0)");
}

export async function waitForProgramInput(page) {
  await expect(page.locator("#status_io")).toHaveCSS("color", "rgb(0, 238, 0)");
}

export async function expectTapeOperation(page, operation, fileNumber) {
  await test.step(`Waiting for ${operation} on tape file ${fileNumber}`, async () => {
    await page.waitForFunction(
      ({ operation, fileNumber }) => window.tapeEvents.some(
        (event) => event.operation === operation && event.fileNumber === fileNumber,
      ),
      { operation, fileNumber },
      { timeout: 30_000 },
    );
  });
}

export async function answerProgram(page, text) {
  await waitForProgramInput(page);
  await setCpuSpeed(page, 1);
  await sendTekText(page, `${text}\r`);
  await setCpuSpeed(page, 100);
}

export async function attachScreen(page, name) {
  const path = test.info().outputPath(`${name}.png`);
  await page.screenshot({ path });
  await test.info().attach(name, { path, contentType: "image/png" });
}

export async function countLitPixels(page, top = 0) {
  return page.locator("#screen").evaluate((canvas, top) => {
    const pixels = canvas.getContext("2d")
      .getImageData(0, top, canvas.width, canvas.height - top).data;
    let count = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index] !== 0 || pixels[index + 1] !== 0 || pixels[index + 2] !== 0) {
        count += 1;
      }
    }
    return count;
  }, top);
}

export async function sendTekText(page, text) {
  await page.evaluate(async (characters) => {
    for (const character of characters) {
      await tek.fcnkey_action(character.charCodeAt(0));
      // Allow the keyboard scan to observe key-up, including between repeated keys.
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }, text);
}

export async function setCpuSpeed(page, target) {
  const button = page.locator("#cpuspeedval");
  const value = `x${target}`;
  for (let attempts = 0; attempts < 8; attempts += 1) {
    if (await button.inputValue() === value) {
      return;
    }
    await button.click();
  }
  throw new Error(`Could not set CPU speed to ${value}`);
}
