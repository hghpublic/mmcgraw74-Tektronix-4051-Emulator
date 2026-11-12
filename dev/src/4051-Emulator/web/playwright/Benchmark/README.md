# Benchmark browser autoplay

See [Benchmark analysis and autoplay design](./Benchmark-analysis.typ) for the
program workflows, clock interfaces, reference limitations, and recorded evidence.

This npm workspace imports all seven original files from
[Benchmark](../../Benchmark/) through the Storage dialog of the unmodified
[web emulator](../../4051-Emulator/). Original programs, result files, and emulator
sources are read-only inputs. Tests use fresh browser pages; tape writes remain
in browser memory and disappear when the page closes.

## Run from the repository root

Requirements: Node.js 20+, Python 3, and an already installed Playwright Chromium.

```sh
npm ci
npm run test:benchmark
```

Dependencies, the lockfile, and `node_modules/` are shared at the repository root.
The project-local npm cache is `.npm-cache/` for root invocations. These commands
do not install browsers or change account/global settings. If Chromium is
missing, arrange an explicitly authorized browser installation first.

Run all registered browser suites, including SimpleDemo:

```sh
npm test
```

Open the Benchmark interactive UI, using a separate port if SimpleDemo's UI is
already on 9323:

```sh
npm run test:benchmark:ui -- --ui-host=127.0.0.1 --ui-port=9324
```

Open the displayed local URL and select a test to play it. For a normal headed
run or just the seven-speed sweep:

```sh
npm run test:benchmark -- --headed
npm run test:benchmark -- --grep "speed sweep"
```

The shared read-only static server binds to `127.0.0.1:8876`. An existing local
server can be reused; CI starts its own. No external service or database is used.

## Programs and coverage

| File | Supplied content | Autoplay behavior |
| --- | --- | --- |
| 1 | Benchmark selector | Answer uppercase `Y` to load 2; separately answer `N` to load 3. |
| 2 | Emulator benchmark | Press RETURN at each input, with actual CPU speeds x1, x2, x5, x10, x20, x50, x100; validate all seven newly written timing rows and the end marker in file 10. |
| 3 | 4050 calculation benchmark | Start through the `N` menu branch, press RETURN, wait for idle, and capture the resulting display. Runs accelerated, not as a stopwatch certification. |
| 4 | MFM benchmark | Explicitly skipped: requires the MFM `!TIME` clock ROM, not the emulator's GPIB clock at address 2. |
| 10 | Historical emulator results | Import unchanged, then allow program 2 to replace the browser-memory copy. Never compare host-dependent timings against these historical values. |
| 11 | TLIST | Wait for the LAST entry at 12, press RETURN, and verify program 1 reloads. |
| 12 | LAST marker | Verify import and its actual directory-listing operation. |

### Known reference limitations

The user selected unchanged-reference behavior, not in-memory BASIC corrections.

Program 2 selects file 10 at line 170, writes seven measurements and `|EOT|`
there, then selects **file 51** at line 520 for readback. File 51 is not among the
supplied assets. Runtime observation shows repeated empty `gpibInput` calls on
51, BUSY remaining lit, and no printed results table. The sweep test explicitly
checks this known loop, carries a `known-limitation` annotation, and captures it.
A passing sweep test means measurements were generated and this limitation was
reproduced; it does **not** mean the original program terminates normally.

Program 4 is listed as a skipped test rather than faking the MFM clock, changing
its CALLs, or claiming complete benchmark coverage. Its supplied source is still
imported. No source patches, generated tape-file aliases, or clock stubs are used.

## Evidence and limits

Each executed test saves a 3200 x 2400 PNG and `tape-operations.json` under
the Git-ignored [test-results](./test-results/) directory. Named stage screenshots
cover the menu, directory, speed prompts/results, and known readback loop.
The sweep also attaches `benchmark-results.txt`, containing the actual new
browser-memory file 10. The underlying emulated canvas remains 1046 x 830.

Import, startup, tape observation, key-release gaps, speed selection, and
screenshots use [shared helpers](../support/emulator.mjs) and
[configuration](../support/config.mjs), also used by SimpleDemo. Storage observers
call the original methods without reading/consuming GPIB output buffers. Menu
answers hold the emulator's Shift key to preserve uppercase `Y`/`N`; subsequent
RETURN is sent after Shift is released.

The runner synchronizes on ROM startup, input readiness, actual file operations,
and newly written result rows rather than fixed prompt delays. At each timing
input, speed is set **before** RETURN and left unchanged until that row is written.
It does not enforce absolute timings or monotonic speedup: wall-clock results
depend on the host/browser load. The calculation-only test has output/activity
and idle checks plus a screenshot, not numerical variable inspection or OCR.
Uncaught browser errors and unexpected dialogs fail every executed test.
