# Phase 0 Baseline Summary

## Scope and status

This is the Phase 0 baseline for migrating the browser Tektronix 4051 emulator to native Rust/egui. The frozen-specification workflow is not complete: character-size rendering, printer output, reliable physical host-key input, and GPIB byte/EOI transcripts remain unverified. Vector rendering has been captured under all three display-granularity settings. The test-case bundle is a draft and must not be treated as a full parity sign-off.

The active Rust module is the single Cargo package `tek4051-emulator`, rooted at `dev/src/4051-Emulator/rs/4051-Emulator-rs`. Its `tests/` path is the conventional Cargo integration-test source root; no integration tests existed before the draft baseline was added.

## Reference revision and launch

- Git revision: `b33e20fb3ef41a1db114b602a29a9ba8ce3fdb48`
- Reference path: `dev/src/4051-Emulator/web/4051-Emulator`
- Launch command: `python3 -m http.server 8765 --directory dev/src/4051-Emulator/web/4051-Emulator`
- URL: `http://127.0.0.1:8765/Tektronix4051.html`
- `Tektronix4051.html` SHA-256: `34a475e5bf001766816f32a0fad447c9a1d7190089ef58a9dfe48c0cfd44f6d9`
- `prog.txt` SHA-256: `f472fc9080151e23d012fd0d1ace2e16904730eb57dc3d9f72a027e144d3b6d1`

The page returned HTTP 200 and loaded in the local browser. Startup, Start/Pause, Reset, display controls, GPIB add/remove, single-file import, tape save/select, Auto Load, and page-reload storage behavior were observed. The browser reported one existing malformed select/input markup warning. No inherited web source or asset was changed.

## Module scope and external boundaries

| Module | Decision | Reason |
|---|---|---|
| `tek4051-emulator` | In scope | This is the Rust target for the user-facing emulator and its machine, display, keyboard, GPIB, tape, and peripheral behavior. |
| Browser reference | Behavior oracle | The static web application is the source of expected behavior; it is not a migrated service module. |

Observed user-facing boundaries are browser page load, front-panel clicks, host/panel keyboard events, GPIB dialog actions, file-picker import, tape-file selection, and Auto Load. The exact source handlers are cataloged in `tests/test-cases/test-cases.md`.

## Test inventory at baseline capture

The following inventory describes the 12 November 2026 capture, not the current repository. The `npm test` result refers to the inherited web-source package, not the repository-root workspace command. The newer root Playwright workspaces and separate Rust test files must be reconciled with this draft before it is frozen; see the graded migration guide for current status.

- No web integration/E2E/regression test files were found.
- `npm test` was run and failed as expected: `Error: no test specified` (exit 1).
- Existing Rust coverage consists of 39 inline unit tests in the CPU/machine source modules. No Rust `tests/` directory or end-to-end harness existed.
- No integration-test framework, fixtures, mocks, or container conventions are established. The baseline bundle uses Cargo's conventional `tests/` root and contains no generated test code.

## Captured behavior

- Initial page title matches the reference; Start is enabled while Pause, GPIB, and Storage are disabled. Speed starts at `x1`; scaling starts enabled; the default granularity and mute controls are recorded in the expectations fixture.
- Start enables Pause/GPIB/Storage and sets the POWER indicator to `rgb(0, 238, 0)`. Pause reverses those enabled states and sets POWER to `rgb(0, 85, 0)`.
- Reset while paused leaves a 1046×830 canvas with zero non-black pixels.
- Speed values cycle `x1 → x2 → x5 → x10 → x20 → x50 → x100 → x1`; scaling toggles `⇲ → ⊞ → ⇲`; granularity cycles `⊚ → ⊙ → ⊛ → ⊚`; audio button label toggles `♫ mute → ‼ mute → ♫ mute`.
- Default GPIB registry: address 1 Program loader, 2 Real time clock, and 5 4924 tape drive. Adding Printer at address 6 and removing it returns the exact default list. Adding Printer at occupied address 1 is ignored without a visible message.
- Importing and saving the supplied vector program as file 1 (ASCII/Program, description `Vector graphics demo`) produces size 235. Selecting it and pressing Auto Load runs `FIND @5:1`, `OLD @5:`, `RUN`. The captured canvas metrics are in `testdata/expectations/vector-program-display.json`.
- With no tape program, Auto Load displays `NO PROGRAM FOUND - MESSAGE NUMBER 59`; the screenshot was captured immediately after the command sequence. The emulator remains running and this transient output can later clear.
- Saving a tape file then reloading the browser clears the tape-file list. This runtime observation confirms the previously selected in-memory storage policy.
- Importing a zero-byte file leaves the viewer empty and the file list empty, with no visible error notification. This is an observed reference limitation, not the desired Rust error UX.
- The supplied vector program was rerun at CPU speed `x100` under all three display-granularity control values, then paused before measuring the canvas. Exact pixel counts and RGB values are in `testdata/expectations/display-granularity.json`. The `⊙` and `⊛` vector captures match; the vector-only program cannot test the character-size distinction of `⊛`.

## Draft test inventory

| Category | New cases | Existing-covered |
|---|---:|---:|
| happy-path | 4 | 0 |
| boundary | 3 | 0 |
| special-input | 0 | 0 |
| failure | 3 | 0 |
| **Total** | **10** | **0** |

Fixtures: `testdata/inputs/prog.txt` (byte-identical copy of the supplied program), `testdata/inputs/empty.txt` (zero bytes), `testdata/expectations/control-cycles.json`, `testdata/expectations/vector-program-display.json`, `testdata/expectations/no-program-autoload.json`, and `testdata/expectations/display-granularity.json`.

The required field checks and fixture paths pass. Full generic entry-point coverage is not claimed: deterministic startup/settings controls have no external dependency-failure mapping, and the physical host-key path has not been verified. This is a draft, not a frozen baseline.

## Infrastructure decision table

No external service dependencies were found; GPIB devices, RTC, tape store, printer adapter, and audio are in-process simulations. The table contains zero dependency rows. The user asked whether the environment-mode choice can be deferred and changed later. It can; `mock` is recorded provisionally as metadata only. After freezing, a change requires an unfreeze, edit, revalidation, and refreeze. No `infra/` directory exists.

## Open observations and blockers

- `tek_load()` reads `#loadfile`, but the loaded page contains no element with that ID (`#loadfile` count is 0). The tested tape-import route uses `#importObj`; do not claim the direct `tek_load` route is operable.
- The browser logs one malformed select/input markup warning during page load. The inherited source remains unchanged.
- No supplied print program was identified. A temporary `PRINT @6:"GPIB PRINT TEST"` probe with a connected Printer at address 6 produced no observable output window; the command path and popup behavior were not independently confirmed.
- The mute button's label cycles, but audible beep/click output was not measured.
- Host physical keyboard mapping was attempted but produced no conclusive visible change. Vector rendering is captured in all three granularity modes, but the character-size difference of the third mode has not been exercised.
- GPIB byte/EOI transcripts remain unobserved.
- The storage zero-byte import lacks a visible error; the Rust port should surface an explicit import failure rather than copying that silent behavior unless parity policy is explicitly changed.

## Freeze and next action

Do not freeze this bundle or claim Grade 0 complete. Next, capture character-size rendering, a conclusive printer-output workflow and physical keyboard sequence, and GPIB output/EOI transcripts; then decide whether the resulting cases satisfy the migration's parity boundary. Reconfirm the integration-test environment mode before freezing the infra table.
