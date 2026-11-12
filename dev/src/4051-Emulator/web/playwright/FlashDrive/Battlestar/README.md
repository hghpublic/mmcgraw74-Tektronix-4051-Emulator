# Battlestar browser autoplay

This workspace imports the original
[Battlestar assets](../../../FlashDrive/Battlestar/) into the unmodified
[web emulator](../../../4051-Emulator/). It does not move, rename, or edit the
source files. Every executed test uses a fresh browser page; generated pictures
replace only the browser-memory tape copies.

## Run from the repository root

Requirements: Node.js 20+, Python 3, and an already installed Playwright Chromium.

```sh
npm ci
npm run test:battlestar
```

Dependencies and the lockfile are shared at the repository root. Do not create a
separate dependency installation here. Root invocations use the project-local
`.npm-cache/`; these commands do not install browsers or alter account/global
settings. Missing browsers require an explicitly authorized installation.

For visual control, open the interactive UI, or run a headed test:

```sh
npm run test:battlestar:ui -- --ui-host=127.0.0.1 --ui-port=9325
npm run test:battlestar -- --headed
npm run test:battlestar -- --grep "Cylon Attack"
```

The UI port differs from the SimpleDemo/Benchmark examples to avoid collisions.
The shared read-only static server uses `127.0.0.1:8876`. `npm test` runs all
registered workspaces, including this one.

## Inventory and exercised behavior

There are **91 source files**: 24 ASCII programs, 26 ASCII data files, 40 binary
data files, and one LAST marker. File numbers are 1-90 except 55, plus 119 and
124. The importer retains the original filenames and tape metadata.

| Scenario / source file | What autoplay verifies |
| --- | --- |
| Import | All 91 tape numbers, plus SHA-256 equality between the large binary picture in file 15 and the original source bytes. |
| 2: Vipers on Patrol | Runs the generator; file 12 receives new binary bytes, its title and EOG marker, and visible canvas output. |
| 26: Cylon tanker text | Runs the character generator; file 27 receives nontrivial binary output and EOG, with visible text. |
| 29: Targeting circles | Runs the generator; file 30 receives its title, vector data and EOG, with visible targeting artwork. |
| 8: Cylon conversion | Reads the original Option 30 data in file 7 and writes the converted R12 object to file 9. The source ends after conversion; this is **not** an Option 30 gameplay test. |
| 4: Cylon Attack | Selects device 3 (UDKs), presses UDK8 to start, verifies the read from file 5 and targeting graphics after the screen-erase flash, then uses UDK10 to attempt loading file 1. |
| 119: TLIST | Automatically pages the multi-screen directory, verifies every directory request from 1 through LAST at 124, then presses RETURN and verifies the attempt to load file 1. |

Picture-write checks observe real `gpibWrite` calls, require the resulting byte
count to equal the number of new bytes written, and check end markers where the
source writes them. No program is patched to manufacture these results.

### Explicitly skipped programs

The user selected **unchanged-reference behavior**, not a compatibility rewrite.

- **File 1, BSG1978 demo:** an actual browser run failed during `OLD` at BASIC
  line 3490, `ROPEN 100`. The supplied program contains 4054 Option 30 statements
  that this 4051 cannot parse. The main demo is therefore explicitly skipped,
  not reported as successfully reaching its menu.
- **File 6, Option 30 game:** declares 4054 Option 30 requirements and uses its
  refresh-graphics statements; this is a source-based compatibility skip.
- **File 28, MUSIC:** calls the unsupported `MUSIC` interface. It is explicitly
  skipped; no fake music routine is substituted.

UDK10 in file 4 and RETURN in TLIST both load the original file 1. Their tests
carry `known-limitation` annotations and capture the resulting load barrier.
Passing verifies the navigation attempt, **not** a restored working demo menu.
The BASIC syntax error shown in those captures is expected reference behavior,
not an uncaught JavaScript exception.

### Not covered

Other source tools (11, 13, 31, 37, 39, 52, 62, 71, 76, 77, 79, 82, 84, 87, 89)
are imported but not executed. Many require a second tape at GPIB address 6;
others involve copy/printing or model-specific setup. They are **unverified**,
not all classified as unsupported. This suite is not exhaustive execution of
all 24 programs or all main-demo scenes.

The game scenario does not test aiming, firing, scoring, joystick input, or
sound. Picture checks are not pixel-perfect golden comparisons or numerical
geometry assertions. There is no generic OCR/BASIC-error detector.

## Synchronization and evidence

Startup, imports, loading, keyboard release gaps, speed control, screenshots,
and tape traces reuse the [shared helpers](../../support/emulator.mjs) and
[configuration](../../support/config.mjs). No shared helper changes are needed.
Programs load and commands are entered at the established synchronized speeds;
computation runs accelerated.

TLIST fills the 4051 screen before reaching LAST. The runner observes directory
progress; when requests stop advancing for 500 ms it captures that screen and
presses the real HOME/PAGE key. This is bounded to five screens, and the final
assertions require the LAST marker, complete request sequence, and input-ready
state. It is not a fixed delay followed by an assumed successful listing.

The game graphics wait excludes a nearly solid-green canvas: PAGE briefly
flashes the whole screen green, which must not count as completed targeting
artwork.

Each executed test saves a **3200 x 2400 PNG**; named stages include generated
pictures, the game, directory pages, and the known return-load limitation.
The underlying canvas remains 1046 x 830. Tests also attach:

- `tape-operations.json`: original storage calls and file numbers.
- `picture-writes.json`: aggregate counts of bytes written, avoiding a huge
  per-byte trace.

Storage observers preserve the original calls, receiver and return values.
They never read consuming GPIB output-buffer getters. Uncaught browser errors
and unexpected dialogs fail every executed test.

Screenshots, traces, generated results, and Playwright reports under
`test-results/` / `playwright-report/` are intentionally Git-ignored. The
automation source and this README are not ignored.

## Recorded validation

On 12 November 2026, `npm run test:battlestar` passed 7 scenarios with 3 explicit
compatibility skips. The subsequent repository-root `npm test` exited 0:
17 passed, 4 skipped across Battlestar, Benchmark, and SimpleDemo. Saved images
were checked for actual game graphics, tanker text, paged directory output,
and the expected original-demo syntax barrier.
