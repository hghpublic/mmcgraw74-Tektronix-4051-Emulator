import { expect, test } from "@playwright/test";

const shellPath = "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/";

async function openEditor(page, { directoryPicker, disableDirectoryPicker = false } = {}) {
  if (directoryPicker) {
    await page.addInitScript(({ fileExists, confirmResult }) => {
      window.editorDirectoryPickerOptions = null;
      window.editorDirectoryCalls = [];
      window.editorSavedBytes = null;
      window.editorConfirmMessages = [];
      const fileHandle = {
        createWritable: async () => ({
          write: async (bytes) => {
            window.editorSavedBytes = Array.from(bytes);
          },
          close: async () => {},
          abort: async () => {},
        }),
      };
      Object.defineProperty(window, "showDirectoryPicker", {
        configurable: true,
        value: async (options) => {
          window.editorDirectoryPickerOptions = options;
          return {
            getFileHandle: async (name, fileOptions) => {
              window.editorDirectoryCalls.push({
                name,
                create: Boolean(fileOptions?.create),
              });
              if (!fileOptions?.create && !fileExists) {
                throw new DOMException("File not found", "NotFoundError");
              }
              return fileHandle;
            },
          };
        },
      });
      window.confirm = (message) => {
        window.editorConfirmMessages.push(message);
        return confirmResult;
      };
    }, directoryPicker);
  }
  if (disableDirectoryPicker) {
    await page.addInitScript(() => {
      Object.defineProperty(window, "showDirectoryPicker", {
        configurable: true,
        value: undefined,
      });
    });
  }
  await page.goto(shellPath);
  await expect(page.locator("#run-pause")).toBeEnabled();
  await page.locator("#tools-panel > summary").click();
  await page.locator("#editor-tab").click();
}

test("loads numbered BASIC source and synchronizes line numbers", async ({ page }) => {
  await openEditor(page);

  const picker = page.locator("#basic-editor-load-picker");
  await picker.setInputFiles({
    name: "sample.bas",
    mimeType: "text/plain",
    buffer: Buffer.from("100 PAGE\r\n110 END\r\n"),
  });

  await expect(page.locator("#basic-editor-source")).toHaveValue("100 PAGE\n110 END\n");
  await expect(page.locator("#basic-editor-filename")).toHaveValue("sample");
  await expect(page.locator("#basic-editor-line-numbers")).toHaveText("1\n2\n3");
  await expect(page.locator("#basic-editor-status")).toHaveText("Loaded sample.bas.");
});

test("loads only the description from a complete Flash Drive filename", async ({ page }) => {
  await openEditor(page);

  await page.locator("#basic-editor-load-picker").setInputFiles({
    name: "1      ASCII   PROG [TEST-01] 20.bas",
    mimeType: "text/plain",
    buffer: Buffer.from("10 END\r"),
  });

  await expect(page.locator("#basic-editor-filename")).toHaveValue("[TEST-01]");
});

test("saves the generated file into a selected folder without a filename dialog", async ({ page }) => {
  await openEditor(page, {
    directoryPicker: { fileExists: false, confirmResult: true },
  });

  await page.locator("#basic-editor-filename").fill("[TEST-01]");
  await page.locator("#basic-editor-source").fill("100 PAGE\n110 END");
  await page.locator("#basic-editor-save").click();

  const calls = await page.evaluate(() => window.editorDirectoryCalls);
  expect(calls[0].name).toMatch(/^1 +ASCII +PROG \[TEST-01\] +\d+\.bas$/);
  expect(calls).toEqual([
    { name: calls[0].name, create: false },
    { name: calls[0].name, create: true },
  ]);
  expect(await page.evaluate(() => window.editorDirectoryPickerOptions))
    .toEqual({ mode: "readwrite" });
  expect(await page.evaluate(() => window.editorSavedBytes))
    .toEqual(Array.from(Buffer.from("100 PAGE\r110 END\r")));
  expect(await page.evaluate(() => window.editorConfirmMessages)).toEqual([]);
  await expect(page.locator("#basic-editor-status"))
    .toHaveText(`Saved ${calls[0].name} in the selected folder.`);
});

test("confirms before replacing a same-named output file", async ({ page }) => {
  await openEditor(page, {
    directoryPicker: { fileExists: true, confirmResult: true },
  });

  await page.locator("#basic-editor-filename").fill("Existing");
  await page.locator("#basic-editor-source").fill("10 PAGE\n20 END");
  await page.locator("#basic-editor-save").click();

  const calls = await page.evaluate(() => window.editorDirectoryCalls);
  expect(calls).toHaveLength(1);
  expect(await page.evaluate(() => window.editorConfirmMessages))
    .toEqual([`Replace existing ${calls[0].name}?`]);
  expect(await page.evaluate(() => window.editorSavedBytes))
    .toEqual(Array.from(Buffer.from("10 PAGE\r20 END\r")));
  await expect(page.locator("#basic-editor-status"))
    .toHaveText(`Saved ${calls[0].name} in the selected folder.`);
});

test("keeps an existing output file when overwrite is declined", async ({ page }) => {
  await openEditor(page, {
    directoryPicker: { fileExists: true, confirmResult: false },
  });

  await page.locator("#basic-editor-filename").fill("Existing");
  await page.locator("#basic-editor-source").fill("10 PAGE");
  await page.locator("#basic-editor-save").click();

  expect(await page.evaluate(() => window.editorSavedBytes)).toBeNull();
  await expect(page.locator("#basic-editor-status"))
    .toHaveText("Save cancelled; the existing file was not changed.");
});

test("reports when folder selection is unsupported", async ({ page }) => {
  await openEditor(page, { disableDirectoryPicker: true });

  await page.locator("#basic-editor-filename").fill("No Folder Picker");
  await page.locator("#basic-editor-source").fill("10 PAGE");
  await page.locator("#basic-editor-save").click();

  await expect(page.locator("#basic-editor-status")).toHaveText(
    "BASIC file save failed: Selecting an output folder is not supported by this browser",
  );
});

test("runs editor source through the BASIC input sequence", async ({ page }) => {
  test.setTimeout(60_000);
  await openEditor(page);

  const runMode = page.locator("#basic-editor-run-mode");
  await expect(runMode).toHaveText("Run mode: Keyboard");
  await expect(runMode).toHaveAttribute("aria-pressed", "false");
  await page.locator("#basic-editor-source").fill(
    '100 PAGE\n110 PRINT "HELLO WORLD"\n120 END',
  );
  await page.locator("#basic-editor-run").click();
  await expect(page.locator("#basic-editor-status")).toHaveText(
    "Program commands were sent; the BASIC program may still be running.",
    { timeout: 45_000 },
  );
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 45_000 },
  );
  await expect(page.locator("#error-message")).toBeHidden();
});

test("runs editor source from private temporary tape without changing the Flash Drive", async ({ page }) => {
  test.setTimeout(60_000);
  await openEditor(page);

  await page.locator("#import-files-picker").setInputFiles({
    name: "1      ASCII   PROG EXISTING 7.bas",
    mimeType: "text/plain",
    buffer: Buffer.from("10 END\r"),
  });
  await expect(page.locator("#flash-file-count")).toHaveText("1 stored file");

  await page.locator("#basic-editor-load-picker").setInputFiles({
    name: "1      ASCII   PROG [TEST-01] 22.bas",
    mimeType: "text/plain",
    buffer: Buffer.from("10 print 10+20\r20 end\r"),
  });
  await expect(page.locator("#basic-editor-source")).toHaveValue("10 print 10+20\n20 end\n");
  await expect(page.locator("#basic-editor-filename")).toHaveValue("[TEST-01]");
  const runMode = page.locator("#basic-editor-run-mode");
  await runMode.click();
  await expect(runMode).toHaveText("Run mode: Temporary File");
  await expect(runMode).toHaveAttribute("aria-pressed", "true");

  await page.locator("#basic-editor-run").click();
  await expect(page.locator("#basic-editor-status")).toHaveText(
    "Temporary BASIC file loaded; the program may still be running.",
    { timeout: 45_000 },
  );
  await expect(page.locator("#basic-prompt-status")).toContainText(
    "BASIC prompt detected",
    { timeout: 45_000 },
  );
  await expect(page.locator("#flash-file-count")).toHaveText("1 stored file");
  await page.locator("#flash-drive-tab").click();
  await expect(page.locator("#flash-directory-body tr")).toHaveCount(1);
  await expect(page.locator("#flash-directory-body")).toContainText("EXISTING");
  await expect(page.locator("#error-message")).toBeHidden();
});
