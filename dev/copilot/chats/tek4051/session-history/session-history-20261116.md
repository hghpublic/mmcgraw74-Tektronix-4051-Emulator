# Session history — 2026-11-16

## Testing BASIC commands in the WASM browser

- The user observed that the first characters were missing (`PRINT 2+3` appeared as `nt 2+3`). Reproduced this immediately after Reset: the browser dispatched the full input, but the BASIC display accepted only the suffix and reported a syntax error.
- Repeated after waiting five seconds following Reset. With actual Shift+Equal for `+`, `PRINT 2+3` appeared in full and displayed `5`. Then entered `PRINT "HELLO"` with Shift+Quote and observed the full command and `hello` output, without a BASIC or UI error.
- The result points to entering input before BASIC is ready after Reset, not dropped browser key events: a post-reset wait restored both command entry and execution. The shell currently has no visible BASIC-ready signal. No source code changed.
- The preview server remains running at `http://127.0.0.1:8000/browser/`; the integrated browser is running x1 and currently displays the successful `PRINT "HELLO"` result. A useful follow-up is to decide whether to add startup/readiness guidance to the browser shell.
- The user asked whether their input can be read concurrently with tests. Added a passive, page-local listener for keydown, keyup, and beforeinput events on the emulator surface; it records but does not prevent or modify events. Browser command tests are complete, so input can be observed without injecting test keystrokes; simultaneous typing and command execution would still share the emulator state.
- The user clarified they meant the chat editor and asked about sending visual feedback during tests. Unsent editor text is not visible; submitted chat messages can be incorporated while independent background tests continue. Browser tests that manipulate the same emulator may need pausing if the feedback involves interacting with that page.
- The user emphasized that the native Rust and WASM emulators must share as much code as possible. Treat minimizing duplicated emulator behavior as an architectural constraint for future port work.
- The user asked about an event-based BASIC-ready signal and prohibited fixed timeouts. Inspection found `Snapshot.basic_prompt_ready` and `prompt_request` are currently host-confirmed values, not emulator detection (`emulation/mod.rs`, `gui/app.rs`, `emulation/session.rs`). Recommended tracing a reliable ROM/input-wait condition and publishing readiness transitions from the shared Rust core so native and WASM can consume the same signal; no implementation started.
- The user specified that machine readiness is authoritative and host key presses must be ignored, not queued, while the machine is not ready. They suggested feedback when keys are ignored; recommend starting with a shared readiness status and visible browser/native status message, keeping audio optional. The existing BUSY PIA output is not yet established as a readiness indicator.
- The user asked how Start, Pause, and Reset work. Verified that Start/Pause are host `Command::Run` transitions controlling `HostRuntime`, while Reset resets the existing `Machine`, restarts timing, and preserves whether it was running. These lifecycle transitions clear the manually confirmed prompt-ready flag; `Command::Input` currently applies keyboard frames without checking that flag or the running state.
- The user reported that fast typing still lost keys. A regression test reproduced the cause: `InputSequence` tracked only one held manual press and discarded a distinct key-down arriving before the previous key-up. Updated the sequencer to retain multiple manual key identities and serialize their taps in key-down order, releasing each by its matching identity. The regression test releases keys in reverse order to verify identity matching.
- Test review found the initial reverse-release assertions could not distinguish a misrouted key-up because both releases occurred before advancing time. Strengthened the test to release queued B first, verify active A remains pressed, then release A and verify B appears and eventually releases. Formatting and all 5 input integration tests pass.
- Validation: the new test failed before the fix and passes after it; all 5 `emulation_input` tests, all 9 WASM API tests, the full Rust package suite (268 tests), strict Clippy, and formatting passed. Rebuilt the WASM and tested in a separate browser page: rapid `print 5` produced `5`, and overlapping physical key-downs for `print 6` produced `6`. The original page was preserved.
- `wasm-bindgen-cli` v0.2.130 was missing, so installed it under ignored `target/wasm-tools` and regenerated ignored `pkg/` bindings locally. No dependency manifest changed. The separately specified machine-readiness gate remains unimplemented.

## Continuing the WASM parity implementation

- Continued W3 by extracting the selected-ZIP decoder into the shared `flash_drive_import` module and exposing typed WASM operations for directory metadata, selected loose-file/ZIP import, confirmed transactional archive replacement, exact file/archive export, Auto Load, explicit prompt confirmation, guarded launch, and notices.
- Preserved each success/error's original selection/member order with a shared index sequence. A focused code review caught that parser errors could be reordered relative to later loose-file errors; the sequence now flows through the shared session, native worker/UI, inline native adapter, and WASM adapter. Regression tests verify the ordering and native notices.
- Added WASM tests for sorted/missing metadata and raw lone-surrogate UTF-16, duplicate ZIP-member ordering, partial errors, whole-store rollback and cancellation, exact canonical filename/bytes, byte-identical archive round-trip, Auto Load failure visibility, and launch readiness gates.
- Updated `WASM.md` and the graded parity plan: W0-W3 are complete; W4 browser file pickers/downloads/directory UI and W5 end-to-end sign-off remain open.
- Validation: full native package suite and full no-default-features WASM suite passed; strict all-target/all-feature Clippy, `cargo fmt --check`, `git diff --check`, Typst compilation, WASM-target build, and `wasm-bindgen 0.2.130` generation passed. A Node smoke test exercised import, directory metadata, file/archive export and restore, cancellation, prompt confirmation, and launch.
- Test review identified that the last invalid-file import notice was checked for its message but not error status; added the missing status assertion and reran all 16 no-default-feature WASM library tests successfully, along with formatting and diff checks.
- Generated bindings in `pkg/` and the WASM build in `target/` are Git-ignored. No browser picker/download UI was implemented or tested. Automatic machine BASIC-readiness detection and the host-input gate remain unresolved; no fixed timeout was introduced.

## Completing W4 browser Flash Drive workflows

- Finished the worker-backed ZIP import/archive export flow and chunked archive snapshot transfer; the main-thread session remains the sole emulator owner and rejects mutations while its export snapshot is active.
- Gated WASM-only owned archive snapshot plumbing so native no-default-feature builds do not retain unused-code warnings. Set Playwright to one worker to prevent contention from invalidating the strict 100 ms frame-gap responsiveness checks.
- Validation: native and no-default-feature WASM package test suites passed, strict all-target/all-feature Clippy passed, all 16 Playwright tests passed serially, WASM build passed, formatting and JavaScript syntax checks passed, and the Typst plan compiled. The generated PDF was removed.
- Updated the W4 parity-plan status and handoff. W5 manual/cross-browser evidence and automatic machine BASIC-readiness/input gating remain open; no fixed timeout was introduced.
- Generated `target/`, package `pkg/`, `node_modules/`, and `.npm-cache/` outputs are Git-ignored.

## Organizing platform-specific crate files

- Moved the WASM adapter/tests and browser shell/tests under `platforms/wasm/`, and the macOS plist/packaging script under `platforms/macos/`. Kept the public `crate::wasm` path and shared emulator modules unchanged.
- Moved generated bindings from the crate-local `pkg/` to ignored workspace `target/wasm/pkg/`; updated imports and served the repository root so the browser can access both package assets and generated output.
- Updated the WASM guide, Rust setup guidance, parity/design documents, and handoff references.
- Validation: native and no-default-feature WASM Rust suites, strict Clippy, WASM build and bindgen, formatting, JavaScript/shell syntax, plist lint, Typst compilation, and the moved macOS packaging script passed. Rust Code Quality and Rust Test Quality reviewers found no issues.
- The relocated Playwright suite currently passes 15/16 tests; one export responsiveness assertion fluctuated between 100.0 ms and 133.4 ms against the strict `<100 ms` threshold. Initial 404s from serving only the crate root were fixed by serving the repository root. Do not lower the threshold without evidence.
- Generated bindings and macOS app output under `target/` are Git-ignored.

## Converting the WASM guide to Typst

- Replaced the crate's `WASM.md` with `WASM.typ`, preserving the build/bindgen commands, API descriptions, browser-shell instructions, generated-output location, and test commands.
- Applied the project Typst layout requirements and updated the browser startup error text and current handoff link.
- `typst compile dev/src/4051-Emulator/rs/4051-Emulator-rs/WASM.typ target/WASM.pdf` passed; the generated PDF was removed.

## Relocating the WASM guide

- Moved the guide into the crate-local `docs/WASM.typ` subfolder, adjusted the relative parity-plan path, browser startup guidance, and handoff links.
- The tagged `dev/docs/tek4051/web/design/dvst.typ` file was left untouched.

## Lowercasing the WASM guide filename

- Renamed the crate-local guide to `docs/wasm.typ` and updated its browser startup message and current handoff links. Typst compilation, JavaScript syntax validation, and `git diff --check` passed; the temporary PDF was removed.

## Clearing the stale VS Code warning

- Confirmed `docs/wasm.typ` is the tracked lowercase path and no emulator source files changed. Cleared the VS Code Notifications Center, but Copilot Chat's stale session-change review for `docs/WASM.typ` remains; Keep and Undo attempts fail because the old-path edit has changed. The user stopped a proposed VS Code reload. Do not retry file changes or reload without direction.

## Tracing machine-authoritative BASIC readiness

- Read the current handoff and continued the open WASM readiness/input-gating work. The startup loop at `0xC6B4` checks the display-ready handshake at DisplayX port A (`0x8794`), not BASIC readiness. The recurring `0xF381` path disassembles to a decrement/branch delay loop and is not an input-wait signal.
- Verified that the shared session still exposes only host-confirmed `basic_prompt_ready` and applies `Command::Input` without a readiness check. The real-ROM keyboard test succeeds, but waits 20 seconds as test setup; that timing is not a suitable detector. Direct ROM input tracing confirmed command entry but did not establish a safe readiness condition.
- Ran `cargo test --locked -p tek4051-emulator --test host_runtime keyboard_entered_basic_program_matches_reference_display --quiet` and `cargo test --locked -p tek4051-emulator --test machine startup_probes_rom_packs_and_processes_keyboard_input --quiet`; both passed. No emulator source changes or guessed PC/RAM detector were made. Temporary diagnostics were created under Git-ignored `target/` and removed.
- Next: keep tracing ROM input flow until a reliable machine state/event is established, including prompt and BASIC `INPUT` states; then implement the shared readiness gate and visible ignored-key feedback.

## Detecting transient cursor semantics

- Traced no-ADOT transient marks to ROM PCs `0xC8BE` and `0xC8F7`; their points form a solid 5-by-8 block at logical origin `(16, 755)`, first complete at about 740 ms in a manually stepped ROM trace.
- Replaced the repeated-single-point candidate with full-glyph detection in the emulation layer. Added semantic display events for transient cursor marks, stored marks/vectors, transient vectors, and erase; renderer decay does not signal readiness. An integration test gates an early press, then enters `PRINT 1` through the session and verifies the printed glyph.
- Began replacing native/WASM manual prompt controls with automatic status and ignored-input feedback. The focused detector tests and BASIC command integration test passed; `cargo test --locked -p tek4051-emulator --lib flash_directory_ -- --nocapture` exposed five native UI/launch failures that still need investigation. Formatting has been run; package/WASM/browser suites and Rust reviews remain outstanding.
- User added that remote-terminal mode may display an external prompt without a blinking cursor. Keep this as an architecture constraint: readiness cannot universally require a blinking cursor; investigate semantic prompt output as an alternative signal.
- Next: resolve input-sequence cancellation and native focus regressions, add a readiness path for non-blinking prompts if the remote-terminal workflow requires it, then finish UI/docs and validation.

## Completing automatic BASIC cursor readiness

- Fixed remaining native tests to wait for actual BASIC cursor detection or to seed semantic cursor events when testing independent keyboard/modal behavior. Host presses during Auto Load are now explicitly verified as ignored instead of canceling scripted input while readiness is false.
- Updated the WASM compatibility setter contract and its test: callers cannot manufacture readiness; they may clear it or attach a legacy request ID after shared detection. Updated the WASM guide and shell text to describe automatic transient-cursor readiness, including persistence through the software blink's off phase.
- Reworked the browser launch test to wait for the real running session's cursor signal rather than the removed manual checkbox. Added a running-but-not-ready WASM regression proving early input is neither queued nor delivered and produces a notice.
- User correction recorded: the 6800's DIMSUB-GRAF firmware loop repeatedly draws the Write-Through cursor with a software delay; drawing stops during BASIC execution/tape loading. The detector observes semantic draw commands, not a blink command or physical phosphor decay. Remote-terminal prompts may lack this cursor and need separate semantics.
- Validation passed: native library/session tests (118 + 4), no-default-features WASM tests (26 + 9), all 16 Playwright tests, strict all-target/all-feature Clippy, WASM target build, `wasm-bindgen 0.2.130`, Rust formatting, JavaScript syntax checks, and Typst compilation. Rust Code Quality reported no findings; Rust Test Quality identified and confirmed the running-startup pre-cursor test fix. The first Playwright pass also hit the known strict `<100 ms` frame-gap fluctuation and stale manual-control test; the corrected full run passed without changing the threshold.
- Build, bindgen, browser-test, and generated PDF outputs are under Git-ignored `target/`. Manual native/cross-browser acceptance and remote-terminal readiness semantics remain open.

## Adding a modular BASIC input status

- Added a non-interactive `BASIC` status badge beside the WASM machine status. It remains muted and announces input unavailable until the shared snapshot reports a running, cursor-ready, error-free session; then its background turns green. It returns to muted when the machine is paused.
- Added `basic_input_status.js` as a separate browser component. It owns the badge, existing BASIC readiness message, and ignored-input notice; `flash_drive.js` no longer owns those UI updates. Documented the badge behavior in the WASM guide.
- Extended the browser launch test to assert the unavailable/green states and the green → unavailable → green transition across pause/resume.
- Validation: all 16 WASM browser tests passed on final rerun; JavaScript syntax checks, Typst compilation, and `git diff --check` passed. An earlier full-suite run hit the existing strict `<100 ms` export frame-gap check at 133.3 ms; the threshold was not changed and the full rerun passed.
- Generated browser test output and WASM bindings remain under Git-ignored `target/`. Manual native/cross-browser acceptance remains open.

## Running the WASM emulator

- Started a workspace-root static server on `127.0.0.1:8769` and opened the package-local browser shell. Clicked Start; the emulator reports Running and BASIC input possible. Shell, new status module, and generated WASM artifact all responded successfully.
- Port 8000 was already serving a different root and returned 404 for this workspace shell, so it was left untouched. The preview server remains running on 8769; no source code changed. WASM/build and browser-test files remain under Git-ignored `target/`.

## Debugging WASM rapid and layout-dependent keyboard input

- Fixed a browser input race by removing the zero-delay printable-key fallback. A printable keydown now stays pending for paired `beforeinput` text; key-up, the next keydown, or focus cleanup resolves/clears the pending key if no text arrives. This lets delayed layout text override a physical key mapping such as German `+` producing `*`.
- Added a browser adapter regression that delays the paired `*` text event and verifies the key is mapped as `*`, plus a WASM API mapping assertion for `+` with paired `*`. A real-ROM session test confirms `print 2*3` produces the same output row as literal `print 6` (64 bright pixels); the Rust Test Quality reviewer found no remaining issues.
- The 100 ms sequenced-tap readiness regression passed while entering `PRINT 1`; readiness stayed true during line editing, and the ROM completed the command. No detector change was needed.
- Validation passed: `cargo fmt --all -- --check`; native library, emulation, and keyboard-mapping tests (118 + 5 + 2); no-default-features WASM library/API tests (26 + 9); strict all-target/all-feature Clippy; and both delayed-layout and key-up fallback browser regressions. The final 18-test browser suite passed 16/18; two existing strict `<100 ms` archive responsiveness checks repeatedly measured 100.0–133.3 ms. Their threshold was left unchanged.
- Browser-test reports/build artifacts under `target/` are Git-ignored. The existing WASM preview page was left running and unchanged; a reload is needed to pick up the updated browser input module. Manual physical German-keyboard and rapid-typing acceptance remains open.

## Running the WASM emulator

- Verified the workspace-root preview server responds at `127.0.0.1:8769`, opened the package-local browser shell in the integrated browser, and started the emulator.
- Confirmed the live page reports Running at x1 and BASIC input possible. No source code changed.

## Continuing keyboard verification and Battlestar readiness

- Added a real-ROM regression proving an unshifted German `+` enters as addition, and a 12-key burst regression that checks each serialized code at the keyboard PIA in order. Added a browser-adapter test for ten rapid printable keys. The Rust Test Quality reviewer reported no actionable findings.
- A temporary 5 ms real-ROM test compared transient framebuffer slices for repeated commands and failed (716 versus 424 bright pixels). The screenshot comparison did not establish whether input was incomplete or the raster was captured in a different state, so that test was removed; it was not treated as proof that the sequencer lost input. The deterministic PIA and browser-adapter tests passed.
- Reproduced the reported Battlestar screen at `INPUT A1` (program asset unchanged). The cursor detector remains false. Live snapshots showed the command prompt with BUSY/IO low and the program input prompt with BUSY/IO high; these are raw Keyboard PIA output bits and are not yet established as unique input-wait indicators. No readiness heuristic was added.
- Validation passed: `cargo test --locked -p tek4051-emulator --test emulation --test emulation_input --quiet` (12 tests); `cargo test --locked -p tek4051-emulator --features wasm --lib --test keyboard_mapping --test wasm_api --quiet` (127 + 3 + 9 tests); strict all-target/all-feature Clippy; `cargo fmt --all -- --check`; `git diff --check`; and all 13 WASM input Playwright tests. Playwright output is Git-ignored under `target/browser-input-tests/`.
- Next: confirm whether BUSY+IO may be treated as readiness for BASIC `INPUT` waits, or continue tracing for a more specific shared-core signal. Physical German-keyboard and rapid-typing acceptance remains open.

## Verifying Battlestar INPUT readiness in the rebuilt WASM browser

- Rebuilt the debug WASM library and regenerated web bindings with `wasm-bindgen 0.2.130`; the 13 WASM browser input Playwright tests passed.
- Opened the package-local shell from the workspace-root preview and imported the read-only Battlestar file 4 fixture. The program reached its `INPUT A1` menu with the BASIC status available. Sending `3` and Return through the browser keyboard advanced to the UDK instructions; BASIC status then became unavailable while the game waited for UDK input. This confirms readiness and host input at the program `INPUT` prompt, and the unavailable state during the non-BASIC wait.
- Removed the generated `target/wasm-readiness.pdf`. `git diff --check` passed. Generated build/bindgen/test outputs are Git-ignored under `target/`; no source files were changed in this chat.
- Updated the handoff to replace the stale unresolved question-mark readiness blocker. Next: manual physical German-keyboard/rapid-typing acceptance and separate remote-terminal prompt semantics.

## Debugging sequential Flash Drive imports

- The active browser page showed disabled import controls and an empty directory while a selected-file batch was still processing. After it completed, 91 files appeared and the controls enabled. A second import of file 4 also completed successfully; its controls were disabled only while the worker was active, then re-enabled. The user confirmed the directory populated as processing finished.
- Added a visible processing status that explains why controls are temporarily disabled. Added a browser regression with a delayed worker response to check the busy state and then re-import the same file.
- Validation passed: the focused import and loose-file/ZIP Playwright tests (2 passed), `node --check` for the modified JavaScript files, and `git diff --check`. No emulator source changes beyond the browser UI; generated browser test output remains under Git-ignored `target/`.

## Fixing CPU-speed-dependent BASIC readiness

- Reproduced the reported x1-versus-x50/x100 difference with the real ROM. The 1,024-event display queue overflowed at x50 while repeated transient cursor marks were being emitted, and the readiness detector correctly cleared its state on a dropped batch.
- Added a real-ROM regression that reaches the BASIC prompt at x1, then checks readiness and event-loss status at x50 and x100. It failed before the fix at x50. The display model now coalesces repeated transient cursor positions within pending event batches, clearing that set on non-cursor events, batch drains, explicit clears, and overflow; distinct marks and non-cursor event ordering remain intact.
- Validation passed: the full `tek4051-emulator` package test suite, `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, WASM target check/build, `wasm-bindgen 0.2.130`, and `git diff --check`.
- In the live WASM browser, imported and selected Battlestar file 4; BASIC remained available and Launch selected program stayed enabled after five seconds at both x50 and x100. Restored the separate test page to x1 with the emulator running. Generated WASM bindings/build files under Git-ignored `target/`.
- No CPU speed or UI gating policy changed. Continue physical German-keyboard/rapid-typing acceptance and investigate readiness for remote-terminal prompts without a blinking cursor.
