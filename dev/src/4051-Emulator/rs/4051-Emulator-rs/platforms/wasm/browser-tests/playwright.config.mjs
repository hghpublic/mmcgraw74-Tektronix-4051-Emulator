import { defineConfig } from "@playwright/test";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const crateRoot = fileURLToPath(new URL("../../../", import.meta.url));
const workspaceRoot = resolve(crateRoot, "../../../../../");

export default defineConfig({
  testDir: ".",
  testMatch: "*.spec.mjs",
  workers: 1,
  outputDir: resolve(workspaceRoot, "target/browser-input-tests"),
  use: {
    baseURL: "http://127.0.0.1:8768",
  },
  webServer: {
    command: `python3 -m http.server 8768 --bind 127.0.0.1 --directory ${workspaceRoot}`,
    url: "http://127.0.0.1:8768/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/input.html",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
