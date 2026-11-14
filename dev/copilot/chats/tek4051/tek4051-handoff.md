# Tektronix 4051 Emulator Handoff

Updated: 14 November 2026.

## Start here

Port the existing web Tektronix 4051 emulator to native Rust/egui with feature parity. The unchanged [web implementation](../../../src/4051-Emulator/web/4051-Emulator/) and supplied assets are the behavior reference.

**Status:** the partial machine core and native application can execute one tested keyboard-entered BASIC program. Imported BASIC programs cannot yet run because native GPIB, tape storage/import, and Auto Load are not implemented. No migration grade exit gate has been formally verified.

Read the [workspace instructions](../../../../.github/copilot-instructions.md) and today's [daily history](session-history/) before working. This handoff records current state; historical milestones, investigations, and validation details remain in the daily files, including [14 November](session-history/session-history-20261114.md).

## Decisions and constraints

- Treat inherited upstream files as read-only unless the user explicitly authorizes the particular change. Preserve supplied ROM/program assets and unrelated local work.
- Target native desktop with egui and the accepted eframe integration. Keep the CPU, machine, display model, and runtime independently testable without UI dependencies in their implementation.
- Match established web behavior, including documented limitations; do not silently repair supplied BASIC programs or extend scope to later 4050 models or unsupported R12 SOUND/MUSIC.
- Storage should match the reference's observed in-memory behavior: saved files disappear on page reload. The web README's browser-local-storage claim does not override the reviewed implementation and runtime evidence.
- Screen copy uses the selected native Save As dialog. Other native file-dialog and printer-presentation choices remain open.
- Public-API Rust tests belong in the package `tests/` directory. Private-access tests belong in separate files declared as private `#[cfg(test)]` modules, never mixed into implementation files.
- Follow the evidence-grounded Rust comment, required reviewer, validation, and Typst conventions in the workspace instructions. Alert the user when task changes are Git-ignored.

## Current Rust implementation

The root [Cargo.toml](../../../../Cargo.toml) is a virtual workspace using resolver 3. Its member [manifest](../../../src/4051-Emulator/rs/4051-Emulator-rs/Cargo.toml) defines `tek4051-emulator`, a library and native binary, and declares Rust 1.85. Root and member commands share the root lockfile and `target/`.

| Area | Implemented and evidenced | Remaining limits |
| --- | --- | --- |
| CPU and machine | Selected MC6800 8/16-bit operations/addressing modes, branches, stack operations, SWI/RTI, IRQ/NMI, RAM, PIAs, keyboard state, and bounded stepping. Base/system-overflow ROMs, both backpacks, and all eight slots of both ROM expanders are mapped. | Complete CPU/board coverage, GPIB handshake, differential traces, and representative option-ROM program execution remain open. |
| Startup and BASIC | Bounded tests reach the base-ROM keyboard routine at `0xC88A` and observe real-ROM key press/release handling without CPU register overrides. A runtime test enters `10 PRINT 1` and `RUN` through the Tek keyboard API and compares the stored-pixel mask with the web reference. | This proves one machine/runtime BASIC workflow, not native physical input, general BASIC compatibility, or full boot/display parity. |
| Display and timing | A 1046-by-830 green-channel framebuffer models vectors, character dots, fine/coarse/mixed granularity, phosphor refresh, and erase flash/completion. `HostRuntime` schedules CPU/keyboard callbacks every 10 ms, refresh every 100 ms, and erase completion after 500 ms. | Native presentation, aspect-ratio/scaling parity, responsiveness, and real-time speed calibration need verification. |
| Native shell and input | eframe renders changed display revisions and shows errors. Host key translation and Shift/Control input are wired, including focus-loss cleanup. Buttons cover speed x1 through x100, ten Shift-selectable user keys, five line-editor keys, Auto Number, Step Program, Home/Page, Rub Out, and Break. Synthetic tests exercise mappings, event ordering, timing, and selected ROM delivery. | Physical keyboard/pointer smoke tests and actual editor/program-control workflows remain unverified. Caps Lock UI, start/pause/reset, status indicators, and other remaining controls are incomplete. Machine execution errors stop the runtime; no UI recovery controls exist yet. |
| Screen copy | MAKE COPY and DisplayX output-A bit `0x20` falling edges capture snapshots. The app writes full-resolution RGBA PNG via `rfd` and `image`; it displays success, cancellation, and errors and excludes modal time from emulation. Tests cover event-time pixels through PNG decode, timing, erase/copy ordering, overflow, and painted overflow warnings. | Native Save As interaction and overwrite confirmation have not been manually verified. |
| Storage and peripherals | Immutable ROM mappings are available. | Native GPIB routing/devices, Loader/RTC/tape workflows, storage/import/export, Auto Load, printer, and UI beep/click audio are not implemented. Imported flash-drive program sets therefore remain unavailable in Rust. |

### Module ownership and entry points

- [machine.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/machine.rs): CPU execution coordination and public machine API.
- [machine_bus.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/machine_bus.rs): memory/ROM mapping, board I/O, bank/PIA constants, and screen-copy snapshot queue.
- [pia.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/pia.rs): PIA types and register behavior; existing `machine::` public type paths remain re-exported.
- [display.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/display.rs) and [host_runtime.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/host_runtime.rs): logical rendering and host scheduling.
- [main.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/main.rs), [front_panel.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/front_panel.rs), and [keyboard_input.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/keyboard_input.rs): native lifecycle, display/file adapters, panel input queue, and host-key translation.
- [tests/host_runtime.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/tests/host_runtime.rs): keyboard-entered BASIC comparison. Public-API tests are in [tests/](../../../src/4051-Emulator/rs/4051-Emulator-rs/tests/); private tests are in separate module files, including [main/tests.rs](../../../src/4051-Emulator/rs/4051-Emulator-rs/main/tests.rs).

### Compatibility boundaries

- `HostRuntime::start()` supplies display readiness. Without it, the bounded reset run waits at `0xC6B4`; this is not evidence that the integrated runtime cannot start.
- Board interrupt routing deliberately uses raw PIA status flags without enable gating, matching the web reference. CPU IRQ masking, NMI priority, held-source suppression, and rearming are tested.
- ROM selectors 2/3 return read-only zero bytes only within `0x8800..0xA7FF`, normalizing the reference's undefined bytes; this is not general JavaScript undefined/NaN or floating-bus parity. Selector 1 maps `0x8800..0x97FF`; selector 4 maps its exact 2 KiB payload at `0x8800..0x8FFF`. Their uncovered tails remain explicit errors, not fallback ROM/zero mappings.
- Pointer presses and intervening releases have a deliberate 100 ms minimum, unlike the web's immediate transitions. Rapid taps are queued and source ordering/modifiers preserved. This needs native interaction validation.
- Screen-copy buffering retains up to eight ordered event-time snapshots. Excess requests are counted and visibly reported without stopping emulation. Native destination choice and modal pause differ from the web's automatic download.

## Latest recorded validation

These are recorded results from the screen-copy increment on 14 November, not fresh runs for this handoff edit:

```sh
cargo fmt --all -- --check
cargo test --locked -p tek4051-emulator --bin tek4051-emulator --test display --test host_runtime
cargo clippy --locked -p tek4051-emulator --all-targets -- -D warnings
cargo build --locked -p tek4051-emulator
```

- The focused test command passed **44 tests**: 34 binary, 4 display, and 6 host-runtime tests. This is not a full-suite total. The final painted-warning assertion also passed its focused rerun and strict Clippy.
- Both required Rust reviewers confirmed their screen-copy findings resolved. The test reviewer additionally reported a passing full-package run; no current full-suite total is asserted here.
- The architecture and graded guide compiled with `typst compile`; temporary PDFs were removed.
- Rust 1.85 compatibility of the current dependency graph and non-macOS platforms have not been tested.
- An earlier native shell screenshot verified window presentation, not input or display parity. A later launch confirmed window discovery, then exited with status 0; do not assume an emulator process is still running.
- Latest recorded root `npm test` result: **26 passed, 6 explicit skips**. This is web-reference evidence, not Rust parity certification.

To launch from the repository root: `cargo run --locked -p tek4051-emulator`. For a macOS app bundle, use [scripts/package-macos-app.sh](../../../src/4051-Emulator/rs/4051-Emulator-rs/scripts/package-macos-app.sh). It writes under `target/macos/`, which is intentionally Git-ignored along with Cargo build output.

## Reference baseline and known limitations

The [draft baseline](../../../docs/tek4051/phase-0/baseline-summary.md) and [test cases](../../../src/4051-Emulator/rs/4051-Emulator-rs/tests/test-cases/test-cases.md) pin reference revision `b33e20fb3ef41a1db114b602a29a9ba8ce3fdb48`. The baseline is **not frozen**. Character-size rendering, reliable physical host-key input, printer output, and GPIB byte/EOI transcripts remain unverified. The test-environment mode is deferred; `mock` is provisional and must be confirmed before freezing.

Root [npm scripts](../../../../package.json) run shared Playwright workspaces for SimpleDemo, Benchmark, Battlestar, and StarWars. Shared support lives in [playwright/support/](../../../src/4051-Emulator/web/playwright/support/). Reconcile this newer coverage with the draft baseline rather than assuming it closes every gap.

Preserve these reference limitations unless separately authorized to adapt them:

- **Import:** the working single-file route is the Storage dialog. Direct `tek_load()` references a missing `#loadfile`; empty imports show no visible error. The protected web source also has a malformed select/input warning.
- **Benchmark:** program 2 writes seven timing records and EOT to file 10, then loops on missing file 51 with BUSY lit; tests assert that known loop. Program 4's unavailable MFM `!TIME` scenario is skipped. Timings are host-dependent, not certification thresholds. See [Benchmark analysis](../../../src/4051-Emulator/web/playwright/Benchmark/Benchmark-analysis.typ).
- **Battlestar:** main file 1 fails at Option 30 `ROPEN` line 3490; it, game 6, and MUSIC utility 28 are skipped. Seven scenarios cover imports/integrity and selected picture, UDK, and TLIST flows, not all programs. See [Battlestar README](../../../src/4051-Emulator/web/playwright/FlashDrive/Battlestar/README.md).
- **StarWars:** nine scenarios pass, but file 3's 4054 Option 30 and file 14's observed subscript error remain skips. Preserve file 12's fragmentary drawing. UDK16 is Shift+UDK6, not scancode `0x6F`; full gameplay remains unverified. Keep the TLIST scenario in `tests/directory.spec.mjs`. See [StarWars README](../../../src/4051-Emulator/web/playwright/FlashDrive/StarWars/README.md).
- [SimpleDemo analysis](../../../src/4051-Emulator/web/playwright/FlashDrive/SimpleDemo/SimpleDemo-analysis.typ) distinguishes manual use from synthetic Playwright input; physical host-key mappings are incompletely validated.

## Next actions

1. Verify native keyboard/pointer input, Shift functions, editor/program controls, speed selection, and both screen-copy sources, including Save As cancellation/overwrite. Record observed behavior separately from synthetic-test evidence.
2. Complete remaining Grade 0 observations, reconcile browser suites with the draft baseline, confirm the test-environment mode, and freeze only when the agreed evidence is complete. These gaps remain open while implementation proceeds.
3. Continue remaining front-panel/lifecycle controls with focused tests; implement native GPIB routing/default devices, numbered tape records and import, then Auto Load. Compare a supplied program's `FIND @5`, `OLD @5`, and `RUN` flow to the reference.
4. Expand CPU/board differential traces and option-ROM/peripheral coverage. Reassess the [graded migration guide](../../../docs/tek4051/web/design/graded-migration-guide.typ) exit gates rather than equating implementation with sign-off.

The [architecture design](../../../docs/tek4051/web/design/design.typ) separates current implementation from planned components. The [migration log](../../../docs/tek4051/migration-log.typ) contains earlier core/ROM decisions and evidence; use daily history for subsequent increments and asset-digest records.

Documentation review follow-up: the phased plan, migration log, baseline summary, and design now distinguish current implementation from historical checkpoints and identify Auto Load as work still required. Their updated Typst files compiled successfully. The draft baseline test specification's original test-inventory metadata remains historical and must be reconciled before freezing.

## Session environment caution

The user reported that a **new chat starts with sandboxing enabled**, while this older chat reported disabled despite repeated `/sandbox enable`. The cause is unresolved; no settings were changed during investigation. Verify the effective policy in the chat being used with `/sandbox policy`; neither this handoff nor another chat's result establishes its sandbox state.
