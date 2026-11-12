import { defineConfig } from "@playwright/test";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDirectory = fileURLToPath(new URL("../../4051-Emulator", import.meta.url));
const baseURL = "http://127.0.0.1:8876";

export function emulatorConfig(directory) {
  return defineConfig({
    testDir: resolve(directory, "tests"),
    testMatch: "**/*.spec.mjs",
    fullyParallel: false,
    workers: 1,
    timeout: 120_000,
    expect: { timeout: 30_000 },
    reporter: "list",
    outputDir: resolve(directory, "test-results"),
    use: {
      baseURL,
      browserName: "chromium",
      headless: true,
      viewport: { width: 1600, height: 1200 },
      deviceScaleFactor: 2,
      screenshot: "on",
      actionTimeout: 15_000,
    },
    webServer: {
      command: "python3 -m http.server 8876 --bind 127.0.0.1",
      cwd: appDirectory,
      url: `${baseURL}/Tektronix4051.html`,
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
    },
  });
}
