import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { deflateRawSync } from "node:zlib";

const zipFixture = Buffer.from(
  "UEsDBBQAAAAIAOMxcF07cVuWBQAAAAMAAAAnAAAAZm9sZGVyLzYgICAgICBBU0NJSSAgIFBST0cgWmlwIEltcG9ydCAzc/VzAQBQSwECFAMUAAAACADjMXBdO3FblgUAAAADAAAAJwAAAAAAAAAAAAAAgAEAAAAAZm9sZGVyLzYgICAgICBBU0NJSSAgIFBST0cgWmlwIEltcG9ydCAzUEsFBgAAAAABAAEAVQAAAEoAAAAAAA==",
  "base64",
);

function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
    }
  }
  return (value ^ 0xffffffff) >>> 0;
}

function largeZipFixture(contentBytes = 8 * 1024 * 1024) {
  const contents = Buffer.alloc(contentBytes);
  const compressed = deflateRawSync(contents, { level: 1 });
  const checksum = crc32(contents);
  const localEntries = [];
  const centralEntries = [];
  let localOffset = 0;

  for (let number = 1; number <= 8; number += 1) {
    const name = Buffer.from(`${number}      ASCII   PROG Large ${contents.length}`);
    const local = Buffer.alloc(30 + name.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(contents.length, 22);
    local.writeUInt16LE(name.length, 26);
    name.copy(local, 30);
    localEntries.push(local, compressed);

    const central = Buffer.alloc(46 + name.length);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(8, 10);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(compressed.length, 20);
    central.writeUInt32LE(contents.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(localOffset, 42);
    name.copy(central, 46);
    centralEntries.push(central);
    localOffset += local.length + compressed.length;
  }

  const centralDirectory = Buffer.concat(centralEntries);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(8, 8);
  end.writeUInt16LE(8, 10);
  end.writeUInt32LE(centralDirectory.length, 12);
  end.writeUInt32LE(localOffset, 16);
  return Buffer.concat([...localEntries, centralDirectory, end]);
}

async function measureUiFrames(page, action) {
  await page.evaluate(() => {
    let active = true;
    let previous = performance.now();
    window.uiFrameGaps = [];
    window.stopUiFrameMeasure = () => {
      active = false;
    };
    const measure = (now) => {
      if (!active) {
        return;
      }
      window.uiFrameGaps.push(now - previous);
      previous = now;
      requestAnimationFrame(measure);
    };
    requestAnimationFrame(measure);
  });
  await action();
  await page.evaluate(() => window.stopUiFrameMeasure());
  return page.evaluate(() => Math.max(0, ...window.uiFrameGaps));
}

test.beforeEach(async ({ page }) => {
  await page.goto(
    "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/",
  );
  await expect(page.locator("#run-pause")).toBeEnabled();
  await page.locator("#tools-panel > summary").click();
});

async function setFiles(page, selector, files) {
  await page.locator(selector).setInputFiles(files);
}

async function downloadedBytes(page, buttonSelector) {
  const downloadPromise = page.waitForEvent("download");
  await page.locator(buttonSelector).click();
  const download = await downloadPromise;
  const stream = await download.createReadStream();
  const chunks = [];
  for await (const chunk of stream) {
    chunks.push(chunk);
  }
  return {
    name: download.suggestedFilename(),
    bytes: Buffer.concat(chunks),
  };
}

async function clickEmulatorControl(page, selector) {
  await page.locator("#controls-tab").click();
  await page.locator(selector).click();
  await page.locator("#flash-drive-tab").click();
}

test("imports loose files and ZIPs and displays ordered failures and metadata", async ({ page }) => {
  await setFiles(page, "#import-files-picker", [
    {
      name: "4      ASCII   PROG Loose 4",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("STOP"),
    },
    {
      name: "broken.zip",
      mimeType: "application/zip",
      buffer: Buffer.from([0xff]),
    },
    {
      name: "selected.zip",
      mimeType: "application/zip",
      buffer: zipFixture,
    },
  ]);

  const rows = page.locator("#flash-directory-body tr");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText("4");
  await expect(rows.nth(0)).toContainText("Loose");
  await expect(rows.nth(1)).toContainText("6");
  await expect(rows.nth(1)).toContainText("Zip Import");
  await expect(page.locator("#flash-notices")).toContainText("2 Flash Drive file(s) imported");
  await expect(page.locator("#flash-notices")).toContainText("Import failed for broken.zip");
});

test("reports import progress and allows the same file to be imported again", async ({ page }) => {
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    window.Worker = class extends NativeWorker {
      postMessage(message, transfer) {
        window.setTimeout(() => super.postMessage(message, transfer), 200);
      }
    };
  });
  await page.reload();
  await expect(page.locator("#speed")).toHaveValue("0");

  const file = {
    name: "4      ASCII   PROG Repeated 4",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("END"),
  };
  const importButton = page.locator("#import-files");
  const rows = page.locator("#flash-directory-body tr");

  await setFiles(page, "#import-files-picker", file);
  await expect(page.locator("#flash-status")).toHaveText(
    "Processing 1 selected file(s); controls are disabled until import completes.",
  );
  await expect(importButton).toBeDisabled();
  await expect(rows).toHaveCount(1);
  await expect(importButton).toBeEnabled();

  await setFiles(page, "#import-files-picker", file);
  await expect(page.locator("#flash-status")).toContainText("Processed 1 selected file(s)");
  await expect(rows).toHaveCount(1);
  await expect(importButton).toBeEnabled();
});

test("rejects an oversized ZIP before asking the browser to read its bytes", async ({ page }) => {
  const limit = await page.evaluate(async () => {
    const wasm = await import("/target/wasm/pkg/tek4051_emulator.js");
    await wasm.default();
    return wasm.max_flash_archive_file_bytes();
  });
  await page.evaluate((archiveLimit) => {
    const size = Object.getOwnPropertyDescriptor(Blob.prototype, "size");
    const arrayBuffer = Blob.prototype.arrayBuffer;
    window.zipReadAttempts = [];
    Object.defineProperty(File.prototype, "size", {
      configurable: true,
      get() {
        return this.name === "oversized.zip" ? archiveLimit + 1 : size.get.call(this);
      },
    });
    Blob.prototype.arrayBuffer = function arrayBufferSpy(...arguments_) {
      if (this.name === "oversized.zip") {
        window.zipReadAttempts.push(this.name);
      }
      return arrayBuffer.apply(this, arguments_);
    };
  }, limit);

  await setFiles(page, "#import-files-picker", {
    name: "oversized.zip",
    mimeType: "application/zip",
    buffer: Buffer.from([0x50, 0x4b, 0x03, 0x04]),
  });
  await expect(page.locator("#flash-notices")).toContainText(
    "ZIP exceeds the 128 MiB compressed file limit",
  );
  expect(await page.evaluate(() => window.zipReadAttempts)).toEqual([]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(0);
});

test("downloads numbered files with the canonical name and exact bytes", async ({ page }) => {
  await setFiles(page, "#import-files-picker", [{
    name: "7      ASCII   PROG Export 4",
    mimeType: "application/octet-stream",
    buffer: Buffer.from([0x00, 0x45, 0xff, 0x0a]),
  }]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);
  await page.locator("#export-file-number").fill("7");

  const download = await downloadedBytes(page, "#export-file");
  expect(download.name).toBe("7      ASCII   PROG Export           4");
  expect(download.bytes).toEqual(Buffer.from([0x00, 0x45, 0xff, 0x0a]));
});

test("cancels archive restore without changing storage and round-trips archive bytes", async ({ page }) => {
  await setFiles(page, "#import-files-picker", [{
    name: "9      ASCII   PROG Archive 3",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("END"),
  }]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);
  const before = await downloadedBytes(page, "#export-archive");
  expect(before.name).toBe("tek4051-flash-drive.zip");
  expect(before.bytes.subarray(0, 4)).toEqual(Buffer.from("PK\u0003\u0004", "binary"));

  page.once("dialog", async (dialog) => dialog.accept());
  await setFiles(page, "#restore-archive-picker", {
    name: "invalid.zip",
    mimeType: "application/zip",
    buffer: Buffer.from([0xff]),
  });
  await expect(page.locator("#flash-status")).toContainText("archive restore failed");
  await expect(page.locator("#flash-directory-body tr")).toContainText("Archive");

  page.once("dialog", async (dialog) => dialog.dismiss());
  await setFiles(page, "#restore-archive-picker", {
    name: before.name,
    mimeType: "application/zip",
    buffer: before.bytes,
  });
  await expect(page.locator("#flash-status")).toContainText("Archive restore cancelled");
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);

  page.once("dialog", async (dialog) => dialog.accept());
  await setFiles(page, "#restore-archive-picker", {
    name: before.name,
    mimeType: "application/zip",
    buffer: before.bytes,
  });
  await expect(page.locator("#flash-status")).toContainText("archive restored: 1 file");
  const after = await downloadedBytes(page, "#export-archive");
  expect(after.bytes).toEqual(before.bytes);
});

test("clears every Flash Drive file only after confirmation", async ({ page }) => {
  const rows = page.locator("#flash-directory-body tr");
  const clearButton = page.locator("#clear-flash-drive");
  await expect(clearButton).toBeDisabled();

  await setFiles(page, "#import-files-picker", [
    {
      name: "4      ASCII   PROG Clear First 1",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("END"),
    },
    {
      name: "6      ASCII   PROG Clear Second 2",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("STOP"),
    },
  ]);
  await expect(rows).toHaveCount(2);
  await expect(clearButton).toBeEnabled();

  page.once("dialog", async (dialog) => dialog.dismiss());
  await clearButton.click();
  await expect(page.locator("#flash-status")).toHaveText(
    "Flash Drive clear cancelled; the directory is unchanged.",
  );
  await expect(rows).toHaveCount(2);

  page.once("dialog", async (dialog) => dialog.accept());
  await clearButton.click();
  await expect(page.locator("#flash-status")).toHaveText(
    "Flash Drive cleared: 2 files removed.",
  );
  await expect(rows).toHaveCount(0);
  await expect(page.locator("#flash-file-count")).toHaveText("0 stored files");
  await expect(clearButton).toBeDisabled();
  await expect(page.locator("#launch-file")).toBeDisabled();
});

test("keeps maximum-size ZIP import and archive encoding responsive", async ({ page }) => {
  const archive = largeZipFixture();
  await page.locator("#controls-tab").click();
  await page.locator("#speed").selectOption("0");
  await page.locator("#run-pause").click();
  await page.locator("#flash-drive-tab").click();
  await expect(page.locator("#run-pause")).toHaveText("Pause");
  await expect(page.locator("#run-pause")).toHaveAttribute("aria-pressed", "true");
  const importGap = await measureUiFrames(page, async () => {
    await setFiles(page, "#import-files-picker", {
      name: "maximum-size.zip",
      mimeType: "application/zip",
      buffer: archive,
    });
    await expect(page.locator("#flash-directory-body tr")).toHaveCount(8);
  });
  expect(importGap, `import animation-frame gap was ${importGap.toFixed(1)} ms`).toBeLessThan(100);

  await page.locator("#controls-tab").click();
  await page.locator("#speed").selectOption("6");
  await page.locator("#flash-drive-tab").click();
  await expect(page.locator("#speed")).toHaveValue("6");
  let exported;
  const exportGap = await measureUiFrames(page, async () => {
    exported = await downloadedBytes(page, "#export-archive");
    await expect(page.locator("#flash-status")).toHaveText("Downloaded the Flash Drive archive.");
  });
  expect(exportGap, `export animation-frame gap was ${exportGap.toFixed(1)} ms`).toBeLessThan(100);
  expect(exported.bytes.subarray(0, 4)).toEqual(Buffer.from("PK\u0003\u0004", "binary"));
  await expect(page.locator("#run-pause")).toHaveText("Pause");
});

test("keeps large whole-store archive restore responsive", async ({ page }) => {
  const archive = largeZipFixture(7 * 1024 * 1024);
  await setFiles(page, "#import-files-picker", {
    name: "large-selected.zip",
    mimeType: "application/zip",
    buffer: archive,
  });
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(8);
  const exported = await downloadedBytes(page, "#export-archive");
  page.once("dialog", async (dialog) => dialog.accept());
  const restoreGap = await measureUiFrames(page, async () => {
    await setFiles(page, "#restore-archive-picker", {
      name: "large-export.zip",
      mimeType: "application/zip",
      buffer: exported.bytes,
    });
    await expect(page.locator("#flash-status")).toContainText("archive restored: 8 file");
  });
  expect(restoreGap, `restore animation-frame gap was ${restoreGap.toFixed(1)} ms`).toBeLessThan(100);
});

test("clicking the display focuses keyboard input and echoes typed text", async ({ page }) => {
  await page.locator("#run-pause").click();
  await expect(page.locator("#basic-prompt-status")).toHaveText(
    "BASIC prompt detected; host keyboard input reaches the machine.",
    { timeout: 5_000 },
  );
  const screen = page.locator("#display-surface > canvas:not([hidden])");
  const before = await screen.screenshot();

  await screen.click();
  await expect(page.locator("#keyboard-input")).toBeFocused();
  await page.keyboard.type("PRINT 123456789", { delay: 80 });

  await expect.poll(async () => !(await screen.screenshot()).equals(before), {
    timeout: 5_000,
  }).toBe(true);
});

test("launches an eligible program only after running and cursor detection", async ({ page }) => {
  await setFiles(page, "#import-files-picker", [{
    name: "3      ASCII   PROG Launch 3",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("END"),
  }]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);
  const basicInputStatus = page.locator("#basic-input-status");
  await expect(basicInputStatus).toHaveAttribute("aria-label", "Machine stopped");
  await expect(basicInputStatus).toHaveCSS("background-color", "rgb(48, 59, 71)");
  await expect(page.locator("#launch-file")).toBeDisabled();
  await expect(page.locator("#auto-load")).toBeDisabled();

  await clickEmulatorControl(page, "#run-pause");
  await expect(page.locator("#run-pause")).toHaveText("Pause");
  await expect(page.locator("#run-pause")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#basic-prompt-status")).toHaveText(
    "BASIC prompt detected; host keyboard input reaches the machine.",
    { timeout: 5_000 },
  );
  await expect(basicInputStatus).toHaveAttribute("aria-label", "BASIC prompt detected");
  await expect(basicInputStatus).toHaveCSS("background-color", "rgb(27, 81, 55)");
  await page.locator("#keyboard-input").focus();
  await page.keyboard.down("a");
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
  await expect(basicInputStatus).toHaveAttribute("aria-label", "BASIC prompt detected");
  await page.keyboard.up("a");
  await clickEmulatorControl(page, "#run-pause");
  await expect(page.locator("#run-pause")).toHaveText("Start");
  await expect(page.locator("#run-pause")).toHaveAttribute("aria-pressed", "false");
  await expect(basicInputStatus).toHaveAttribute("aria-label", "Machine stopped");
  await expect(basicInputStatus).toHaveCSS("background-color", "rgb(48, 59, 71)");
  await clickEmulatorControl(page, "#run-pause");
  await expect(page.locator("#run-pause")).toHaveText("Pause");
  await expect(page.locator("#basic-prompt-status")).toHaveText(
    "BASIC prompt detected; host keyboard input reaches the machine.",
    { timeout: 5_000 },
  );
  await expect(basicInputStatus).toHaveAttribute("aria-label", "BASIC prompt detected");
  await expect(basicInputStatus).toHaveCSS("background-color", "rgb(27, 81, 55)");
  await page.locator("#flash-directory-body tr").click();
  await page.locator("#launch-file").click();
  await expect(page.locator("#flash-status")).toContainText(
    "Program launch requested for Flash Drive file 3",
  );
});

test("cancels Auto Load and re-enables Flash Drive operations", async ({ page }) => {
  await setFiles(page, "#import-files-picker", [{
    name: "1      ASCII   PROG Auto Load",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("END"),
  }]);

  await clickEmulatorControl(page, "#run-pause");
  await expect(page.locator("#basic-prompt-status")).toHaveText(
    "BASIC prompt detected; host keyboard input reaches the machine.",
    { timeout: 5_000 },
  );
  await page.locator("#auto-load").click();

  const cancelAutoLoad = page.locator("#cancel-auto-load");
  await expect(cancelAutoLoad).toBeVisible();
  await expect(page.locator("#import-files")).toBeDisabled();
  await expect(page.locator("#clear-flash-drive")).toBeDisabled();
  await cancelAutoLoad.click();

  await expect(page.locator("#flash-notices")).toContainText(
    "Cancelled; a partial command may remain in the BASIC editor",
  );
  await expect(cancelAutoLoad).toBeHidden();
  await expect(page.locator("#import-files")).toBeEnabled();
  await expect(page.locator("#clear-flash-drive")).toBeEnabled();
});

test("launches the supplied binary FlyBy program from the Flash Drive view", async ({ page }) => {
  const fileName = "2      BINARY  PROG FlyBy simulator 28928";
  const binaryProgram = await readFile(new URL(
    `../../../../../web/FlashDrive/Opt30Demo/${fileName}`,
    import.meta.url,
  ));
  await setFiles(page, "#import-files-picker", [{
    name: fileName,
    mimeType: "application/octet-stream",
    buffer: binaryProgram,
  }]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);
  await expect(page.locator("#flash-directory-body tr")).toContainText("BINARY");
  await expect(page.locator("#flash-directory-body tr")).toContainText("PROG");

  await clickEmulatorControl(page, "#run-pause");
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected; host keyboard input reaches the machine.",
    { timeout: 5_000 },
  );
  await page.locator("#flash-directory-body tr").click();
  await page.locator("#launch-file").click();

  await expect(page.locator("#flash-status")).toContainText(
    "Program launch requested for Flash Drive file 2",
  );
  await expect(page.locator("#flash-status")).not.toContainText(
    "not an ASCII or BINARY PROG",
  );
});

test("runs the Pi benchmark RTC requests without a WebAssembly runtime error", async ({ page }) => {
  const fileName = "10     ASCII   PROG Pi to length     3000";
  const program = await readFile(new URL(
    `../../../../../web/FlashDrive/Utilities/${fileName}`,
    import.meta.url,
  ));
  await setFiles(page, "#import-files-picker", [{
    name: fileName,
    mimeType: "application/octet-stream",
    buffer: program,
  }]);
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);

  await clickEmulatorControl(page, "#run-pause");
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected; host keyboard input reaches the machine.",
    { timeout: 5_000 },
  );
  const inputStatus = page.locator("#basic-input-status");
  const keyboardInput = page.locator("#keyboard-input");
  await page.evaluate(() => {
    window.rtcIoTransitions = [];
    const indicator = document.querySelector("#status-io");
    const observer = new MutationObserver(() => {
      window.rtcIoTransitions.push(indicator.getAttribute("aria-label"));
    });
    observer.observe(indicator, { attributes: true, attributeFilter: ["aria-label"] });
  });

  async function sendBasicCommand(command, timeout = 10_000) {
    await keyboardInput.focus();
    await page.keyboard.type(command, { delay: 80 });
    await page.keyboard.press("Enter");
    await expect(inputStatus).toHaveAttribute("aria-label", "BASIC prompt not detected", {
      timeout: 5_000,
    });
    await expect(inputStatus).toHaveAttribute("aria-label", "BASIC prompt detected", {
      timeout,
    });
  }

  await sendBasicCommand("FIND@5:10");
  await sendBasicCommand("OLD@5:");
  await page.evaluate(() => {
    window.rtcIoTransitions = [];
  });
  await sendBasicCommand("RUN");
  await sendBasicCommand("1", 30_000);
  await expect.poll(
    () => page.evaluate(() => window.rtcIoTransitions),
    { timeout: 30_000 },
  ).toEqual(expect.arrayContaining(["I/O on", "I/O off"]));
  await expect(page.locator("#status-io")).toHaveAttribute("aria-label", "I/O off", {
    timeout: 30_000,
  });
  await expect(page.locator("#error-message")).toBeHidden();
});
