# SimpleDemo browser automation

This Playwright workspace package imports and exercises the programs in
`../../../FlashDrive/SimpleDemo` using the unmodified web emulator in
`../../../4051-Emulator`.

The harness does not edit emulator files or FlashDrive files. It starts a
read-only static server on `127.0.0.1:8876`, imports the original FlashDrive
filenames through the Storage dialog, and runs programs by sending the
emulator's keyboard events.

## Requirements

- Node.js 20 or newer
- Python 3
- Chromium installed for Playwright

## Run

From the repository root:

```sh
npm ci
npm test
```

Dependencies and the lockfile are shared at the repository root; do not install
separate dependencies in each example directory. `npm ci` does not install
Chromium; these commands use the already installed Playwright browser.
The project-local npm cache is `.npm-cache/` when commands run from the root.

Run only SimpleDemo, or open its interactive test UI:

```sh
npm run test:simpledemo
npm run test:ui
```

New example folders under `web/playwright/FlashDrive/` can contain their own `package.json`
with a unique name and `test` script; the root workspace includes them
automatically. Shared test dependencies belong in the root manifest.

The tests verify imported file numbers, actual tape operations, input readiness,
generated artwork, and absence of uncaught browser errors. `Simple Demo` must
read files 23 and 9 and load program 2; `TLIST` must reach the LAST entry and
reload program 1. `Simple create` must rewrite its in-memory data file.
They do not compare exact pixel images.

The runner waits for ROM startup before typing, sends `FIND`, `OLD`, and `RUN`
separately, and waits for command completion or the program's I/O input lamp
instead of fixed one-second pauses. A 50 ms key-up gap lets the keyboard scan
detect releases, including repeated digits in file number 119.
These helpers and the browser/server configuration are shared with
[Benchmark autoplay](../../Benchmark/README.md) through
[the support directory](../../support/).

## Visual inspection

The browser viewport is 1600 x 1200 CSS pixels with 2x pixel density.
Each test saves a 3200 x 2400 PNG screenshot in `test-results/`.
This improves inspection without changing the emulator's 1046 x 830 canvas
or its rendering behavior; extra screenshot pixels do not add emulated detail.
Playwright UI may scale its preview to fit: open the saved PNG at full size
to inspect the captured resolution.

Stage screenshots show Mickey Mouse, Death Star, and the directory listing.
Each test also saves `tape-operations.json` recording real tape operations;
the observers call the original storage methods without consuming GPIB buffers.
ROM-startup and tape-operation waits appear as single waiting steps in Playwright
UI; they fail only if their condition is not met within 30 seconds. Earlier `expect.poll`
checks could show temporary failed attempts even when the enclosing test passed.

The earlier missing `FIND` prefix and BASIC messages 59 and 36 were caused by
typing before ROM startup completed, then sending later text without waiting
for the program's input prompt. Readiness and tape-operation assertions now
guard those paths. The suite does not perform generic OCR/error detection on
the canvas.

## Scope

The test package and generated test artifacts stay under this directory;
shared harness code lives under `web/playwright/support/`.
Shared dependencies are in the root `node_modules/`, managed by the root
`package.json` and `package-lock.json`; no account-level npm settings are changed.
`4051-Emulator` and `FlashDrive` are read-only inputs.
