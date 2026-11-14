#set par(justify: true)
#set page(
  margin: 40mm,
  header-ascent: 18pt,
  footer-descent: 18pt,
  header: context {
    if counter(page).get().first() > 1 {
      let prior = query(heading.where(level: 1, outlined: true))
        .filter(h => h.location().page() <= here().page())
      let label = if prior.len() > 0 { prior.last().body } else { [] }
      grid(
        columns: (1fr, auto, 1fr),
        gutter: 0pt,
        align: (left, center, right),
        [],
        label,
        [],
      )
    }
  },
  footer: context {
    let page_no = counter(page).display("1")
    if counter(page).get().first() == 1 {
      align(center)[#page_no]
    } else {
      align(right)[#page_no]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Tektronix 4051 Emulator]
  \
  #text(size: 15pt)[Rust/egui Migration Log]
]

#align(center)[
  Historical coverage: project review through the initial machine core and host timing adapter
  \
  Source history: `dev/copilot/chats/tek4051/session-history`
  \
  Rust implementation: `dev/src/4051-Emulator/rs/4051-Emulator-rs`
  \
  Current status updated: 14 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and current status

This log records migration decisions, issues, fixes, and validation evidence for the Rust/egui port. Its detailed milestones and validation record cover work through 13 November; later increments are recorded in the daily session history. Historical observations and test counts below are checkpoint evidence, not current inventory. The web emulator and supplied assets remain the behavior reference and have not been modified.

Rust-owned ROM images live under `dev/src/4051-Emulator/rs/4051-Emulator-rs/roms/` as `base.bin`, `overflow.bin`, `backpack-left.bin`, `backpack-right.bin`, `romexp-left.bin`, and `romexp-right.bin`. The base and overflow image paths changed from their former `_rom` names without changing bytes. The left-backpack image is an exact 2 KiB extraction of the reference's `DERROML` array; the right-backpack image is the exact 8 KiB `DERROMR` array. The 64 KiB left- and right-expander images each concatenate all eight exact 8 KiB arrays returned by their respective reference functions.

The current implementation includes a UI-independent CPU and machine core plus a separate host timing adapter. The core supports selected 8/16-bit instructions and addressing modes, reset/interrupt behavior, 32 KB RAM, immutable base, system-overflow, selector-4/5 left/right-backpack ROMs, selector-6/7 left/right ROM-expander slots, PIA control/data registers, keyboard signaling, board IRQ/NMI routing, display erase/ready signaling, and bounded instruction stepping. Without the host's initial display-ready notification, the ROM waits at `0xC6B4` with interrupts masked. Calling `display_ready()` releases that wait without changing CPU registers; startup executes selector-1 overflow code, probes ignored banks 2/3, reads both backpack headers and slot-0 headers from both expanders, then continues through base-ROM code. Within a 2,000,000-instruction bound after the right-expander probe, the normal startup trace reaches the observed keyboard routine at `0xC88A`; the host runtime schedules keyboard refresh, CPU execution, display updates, and erase completion while the regression verifies that the real ROM records and releases an `A` key event. This does not establish complete boot. The JavaScript `DERROML` image contains 2 KiB although selector 4 is selected across an 8 KiB window; Rust maps the actual bytes at `0x8800..0x9000` and reports the remaining range as unsupported rather than inventing data. The JavaScript `DERROMR` image contains a complete 8 KiB of `0xFF`, which Rust maps read-only across `0x8800..0xA7FF`. Selector 6 maps all eight source-derived `BSXLSLOT1`–`BSXLSLOT8` images and selector 7 maps all eight `BSXRSLOT1`–`BSXRSLOT8` images read-only across the same window, selected by the register address field. Ignored banks retain the explicit read-only zero-byte mapping under DEC-007.

The logical display and native eframe shell are implemented in part, including host and panel input, speed/granularity settings, and screen-copy Save As. One keyboard-entered BASIC program is tested at the machine/runtime boundary. Native interaction, broader CPU differential coverage, GPIB and other peripherals, and option-ROM program execution remain open. No grade exit gate is formally verified; the current design and graded guide describe the implementation and remaining evidence.

#table(
  columns: (1.1fr, 1.2fr, 2.7fr),
  stroke: 0.5pt,
  [*Area*], [*Status*], [*Current evidence*],
  [Platform and persistence], [Decided], [Native desktop; tape storage follows the reference's apparent in-memory behavior],
  [CPU core], [In progress], [Selected instruction families, interrupt vectors, bounded startup/input, and host timing execute; broader differential coverage is open],
  [Machine board], [In progress], [32 KB RAM, base/overflow/backpack/left/right-expander ROMs, explicit ignored banks, PIA/keyboard/interrupt and display signaling; host timing is implemented],
  [Parity baseline], [In progress], [Program run, control cycles, GPIB registry, file import, and in-memory storage evidence captured; printer and several input/peripheral paths remain],
  [UI], [In progress], [Native binary renders the display, accepts host/panel input, and offers screen-copy Save As; physical interaction remains unverified],
)

= Decisions

== DEC-001 - Native desktop target

*Decision:* port the emulator to a native desktop application using egui. The user selected this target.

*Effect:* keep the machine and CPU independent of egui. The package now has a core library and eframe binary; screen-copy Save As is implemented, while other native file and printer presentation decisions remain open.

== DEC-002 - Tape storage behavior

*Decision:* match the web implementation's apparent in-memory tape store rather than adding persistence across reloads.

*Basis:* the README says browser-local storage, while the inspected `Storage.js` uses an in-memory `fileStore` and no browser persistence API. Revisit only if runtime evidence contradicts this behavior.

== DEC-003 - Cargo and Rust source locations

*Decision:* use the repository-root `Cargo.toml` as a virtual workspace manifest and `dev/src/4051-Emulator/rs/4051-Emulator-rs` as the member package and implementation root.

*Effect:* the root workspace uses resolver 3. The member's `Cargo.toml` defines `tek4051-emulator`, Rust 2024 with a minimum Rust version of 1.85, and `[lib] path = "lib.rs"`. Cargo automatically discovers the member's public-API tests under `tests/`; explicit root test targets are no longer needed. Root-level Cargo commands continue to work, sharing one root `Cargo.lock` and ignored `/target/` directory; Rust sources and both manifests remain visible to Git.

== DEC-004 - Compatibility boundary and source protection

*Decision:* preserve behavior established by the current 4051 web implementation and supplied assets. Do not silently expand scope to later 4050-series models or the documented unsupported R12 SOUND/MUSIC behavior.

*Constraint:* inherited web code and assets are read-only unless a specific change is authorized. No web source or asset changes have been made during the migration work recorded here.

== DEC-005 - Evidence required for parity

*Decision:* passing builds and unit tests are necessary but do not establish feature parity. Record observable reference behavior and compare representative program, display, input, GPIB, storage, and peripheral behavior before claiming parity.

== DEC-006 - Preserve reference-specific CPU flags

*Decision:* retain the current web source's NEG carry and CPX overflow behavior rather than silently normalizing them to conventional MC6800 descriptions.

*Basis:* `mc6800.js` sets NEG carry when the result is zero and derives CPX overflow from the subtraction borrow. Focused tests preserve these compatibility behaviors; revisit only with an explicit parity-versus-hardware decision.

== DEC-007 - Explicit ignored-bank byte values

*Decision:* represent source-ignored selectors 2/3 as read-only zero-byte banks only inside `0x8800..0xA7FF`, independently of the expander-address field.

*Basis and boundary:* the web board returns JavaScript `undefined` for these selectors; its word reader shifts/ORs those results to zero, so the startup header probe sees no ROM. The accepted compatibility policy normalizes these byte values to Rust `u8` zero. This is not hardware floating-bus emulation or a promise to reproduce all JavaScript undefined/NaN byte-arithmetic quirks. Other unsupported mappings remain explicit errors; there is no general zero/base-ROM fallback.

= Issue and fix register

== ISS-001 - Empty Cargo manifest

*Status: Resolved.*

The repository-root `Cargo.toml` was initially empty. It first defined the package directly and now defines a virtual workspace. Its member manifest under the implementation directory defines the `tek4051-emulator` Rust 2024 package with minimum Rust version 1.85 and a local `lib.rs` target. Root-level locked check, build, and test commands pass.

== ISS-002 - Storage persistence discrepancy

*Status: Decision recorded; runtime evidence may reopen it.*

The source README's browser-local-storage claim conflicts with the inspected in-memory storage implementation. The user selected the in-memory behavior for the port. No persistent-storage layer has been added.

== ISS-003 - Browser markup warning

*Status: Open; upstream source unchanged.*

The web reference started and responded to Start and Reset during local inspection, but the browser reported a malformed select/input markup warning. This warning has not been corrected because the web implementation is the protected reference; determine whether it affects required behavior during baseline testing.

== ISS-004 - Missing executable parity baseline

*Status: Partially addressed; Phase 0 exit gate remains open.*

A draft baseline now records source revision, launch procedure, the supplied vector program's visible display metrics, lifecycle/settings controls, GPIB registry operations, tape import/selection, empty-file behavior, and the in-memory storage reset on page reload. The vector program was also captured at all three display-granularity settings; exact pixel counts are in the draft baseline fixture. The web package's `npm test` command remains a placeholder rather than a meaningful regression suite. Character-size rendering, printer output, reliable physical-keyboard behavior, and GPIB byte/EOI transcripts are still missing; the bundle is not frozen and does not establish parity.

== ISS-005 - Unsupported-opcode error label

*Status: Resolved.*

The CPU error variant was renamed from `IllegalOpcode` to `UnsupportedOpcode`. This avoids labeling a legal MC6800 instruction as illegal merely because its implementation has not yet been added.

== ISS-006 - RTS stack pointer changed after a failed read

*Status: Resolved and reviewed.*

The original pop path advanced SP before reading the stack byte. If the bus read failed, SP was left advanced; failure on the second byte could also leave a partial update. The implementation now advances SP only after a successful byte read and restores the original SP if the second read of a word fails. Tests cover failure on both RTS stack-byte addresses, and the Rust code reviewer confirmed the fix.

== ISS-007 - Branch wrap test overlapped the reset vector

*Status: Resolved.*

The first wraparound fixture placed an instruction byte at `0xFFFF`, which is also the low byte of the reset vector. The fixture now starts the branch at `0xFFFC` and verifies that a forward branch wraps to `0x0000`.

== ISS-008 - CPU test coverage gaps

*Status: Addressed for the current instruction slice.*

Review identified gaps in arithmetic carry/overflow cases, logical-operation H/C preservation and V clearing, combined branch flags, branch CCR preservation, signed displacement endpoints, BSR/RTS stack failures and CCR preservation, negative BSR displacement, and immediate B load flags. Focused tests were added. The test reviewer found no remaining high-confidence gaps in the reviewed cases.

== ISS-009 - Incomplete CPU and machine coverage

*Status: Open; expected at this migration stage.*

The CPU supports only a subset of instructions and addressing modes. Without host display readiness, the bounded base-ROM reset path reaches a repeatable loop at `0xC6B4`. Subsequent work added keyboard/interrupt handling and an egui shell, but additional instruction coverage, board behavior, peripherals, and native interaction verification remain open.

== ISS-010 - Banked ROM integration is incomplete

*Status: System overflow, ignored banks, left/right-backpack payloads, and all slots in both expanders implemented; remaining mappings stay explicit errors.*

The board latches writes to the bank selector at `0x87C0`. Selector 1 maps U101/U201 (`ROM747`/`ROM748`) at `0x8800..0x97FF`, matching both 2 KB reference chip windows. Selector 4 maps the exact 2 KiB `DERROML` payload at `0x8800..0x8FFF`, independent of the expander-address field; its uncovered selector window remains explicit as `UnsupportedRomBank`. Selector 5 maps all 8 KiB of the exact `DERROMR` array at `0x8800..0xA7FF`, also independent of the expander-address field. Selector 6 maps `BSXLSLOT1`–`BSXLSLOT8` and selector 7 maps `BSXRSLOT1`–`BSXRSLOT8`, each with eight exact 8 KiB payloads read-only across `0x8800..0xA7FF`, selected by the low three register address bits. Their concatenated 64 KiB images have SHA-256 `2f7137f745039e043a004b38e0f814788d8c9951d43f88108b66371859838e40` / `d900890f3c71116bfa2a4430ccc63e7c89383143464253f9c3bc6f439b5ce0c3` and FNV-1a-64 `0xa81f808d9880651e` / `0xf464189c814ef54e`, respectively. Selector 0 retains base-ROM mapping; selectors 2/3 have the explicit ignored-bank mapping in DEC-007. Addresses outside the banked window remain unchanged. Selector 1's unmapped `0x9800..0xA7FF` range and selector 4's uncovered tail remain explicit `UnsupportedRomBank` errors. Startup reads slot-0 headers (`0x4051`) from both expanders and continues into base-ROM code with the CPU interrupt mask clear. Full option-ROM program coverage remains Phase 4.

== ISS-011 - Direct web file-load element is absent

*Status: Observed; inherited reference unchanged.*

The web helper `tek_load()` looks up `#loadfile`, but the loaded page contains no element with that ID. A local browser query returned zero matches. The Storage dialog's `#importObj` path does work and was used for the baseline program; do not treat the direct `tek_load()` route as verified.

== ISS-012 - Empty storage import has no visible error

*Status: Observed; inherited reference unchanged.*

Importing a zero-byte file through the Storage dialog leaves the file selector and viewer empty and shows no visible error notification. Record this as a reference limitation, but the Rust port should surface an explicit import failure unless the parity policy is deliberately changed.

== ISS-013 - Keyboard input and PIA interrupt path

*Status: Machine input and PIA behavior are implemented; host refresh cadence is covered in ISS-020.*

Keyboard press/release and active-low modifier state now feed the U461 PIA input registers. CA1-CA2 and CB1-CB2 edge flags latch according to control-register polarity, and data-register reads clear the flag bits. Board routing follows the web reference's raw status-flag checks, without PIA-enable gating: U565/U461/U265 feed IRQ and U561/U361 feed NMI. CPU stepping preserves the reference's interrupt-mask, NMI-priority, and held-source entry behavior. This is a compatibility choice, not a claim of hardware-correct PIA output emulation. The web reference's keyboard-data changes and periodic CA1 refresh are separate; `HostRuntime` now schedules the refresh independently of key events. A bounded base-ROM test presses `A`, confirms the ROM records it at RAM `0x059C` with key-down state at `0x0597`, then releases it and confirms the state clears. Physical host-key translation, complete boot, and UI integration remain open.

== ISS-014 - Startup display readiness was missing

*Status: Root cause identified and board handshake implemented; the ROM boundary recorded in ISS-010 has since been integrated, but complete boot remains unverified.*

The loop at `0xC6B4` reads DisplayX port A (`0x8794`), XORs `0x04`, and tests `0x0C`: it waits for DRBUSY-0 high and VPULSE-1 low. The reference's `execute_start()` sets input bit `0x04`; `HostRuntime::start()` now does the same with `display_ready()`. A data-selected output-A bit-`0x10` falling edge clears readiness and latches `display_erase_pending()`. The host completes the erase with `display_ready()`, preserving unrelated inputs and clearing the pending request. `HostRuntime` applies the reference's 500 ms delay from elapsed host time; the deterministic core does not substitute instruction counts for elapsed time or auto-complete erases. Logical display rendering was added later; full startup and native presentation parity remain unverified.

== ISS-015 - Empty-bank startup probes

*Status: Resolved for the accepted typed-byte policy; complete boot remains open. The trace described below is an earlier checkpoint, superseded by ISS-018 and ISS-019.*

The original trace stopped at bank 2 after 102,553 completed instructions. DEC-007 now defines zero-byte reads for ignored banks 2/3; the ROM's `LDX` header probe at `0xAA8F` observes zero for both and continues normally. Tests execute the actual word-load instruction and verify both ignored-bank probes in the real startup path. At the later selector-4 checkpoint, the startup regression required its header probe before observing the next explicit error, selector 5 at `0x8800`; the trace had no erase request or IRQ enable. The right-backpack and subsequent normal-startup work were completed later, as recorded in ISS-010 and ISS-019.

== ISS-016 - Left backpack payload is shorter than its selected window

*Status: Supplied payload mapped; the uncovered tail remains an explicit error. The selector-5 startup stop below is an earlier checkpoint, superseded by ISS-010 and ISS-019.*

The web board selects `DERROML` for selector 4 across `0x8800..0xA7FF`, but the reference `Uint8ClampedArray` literal contains 2,048 bytes. Rust stores the exact payload and maps it at `0x8800..0x9000`; reads elsewhere in the selected window return `UnsupportedRomBank` rather than fabricated zeroes. The image fingerprint is `SHA-256 cb16cccbf9f9c45ed2e655e786642c07ebf253caff407912f574af145d59624f` (`FNV-1a-64 0xb1dc383ab2158059`). At this checkpoint, the real startup trace passed the header probe and stopped at selector 5 / `0x8800`.

= Migration milestones

== 2026-11-12 - Web review and migration design

Reviewed the web entry point, CPU/machine, display, keyboard, ROM assets, GPIB devices, storage, UI, and project metadata. Created the architecture design, graded migration guide, and phased migration plan under `dev/docs/tek4051/web/design`. The Typst documents compiled successfully; temporary PDFs were removed.

== 2026-11-12 - Rust package and initial CPU slice

Configured the root Cargo package and implemented a testable CPU foundation with reset-vector handling, NOP, immediate LDAA/ADDA, and extended STAA. The first validation reached 10 focused CPU tests, followed by reviewer-requested assertions and error-path cases.

== 2026-11-12 - CPU expansion and review fixes

Expanded the decoder with selected immediate A/B arithmetic and logical operations, conditional branches, BSR, and RTS. Fixed the RTS stack-read error path, expanded coverage for flags and boundary cases, and corrected the branch wrap test fixture. At this checkpoint, 24 focused CPU tests passed.

== 2026-11-12 - Phase 1 machine-core increment

Added direct, indexed, and extended 8-bit operations, selected 16-bit and unary operations, stack/control instructions, SWI/RTI, and explicit IRQ/NMI service. Added a 32 KB RAM/PIA/base-ROM bus and bounded instruction stepping. Generated a Rust-owned 32 KB base-ROM image from the existing 16 chip arrays without modifying upstream files; SHA-256 is `5b847c350fbd2e19fb609070bf76a929ce4068292a4c6cdc8568e91b2a4df59e`. Review findings led to explicit unsupported bank-read errors and additional IRQ/flag coverage. The web source's NEG/CPX flag behavior was confirmed and intentionally retained for compatibility. The core executes twelve ROM reset instructions; this is not full boot or Phase 1 completion.

== 2026-11-12 - Partial Phase 0 baseline capture

Pinned the web reference to revision `b33e20fb3ef41a1db114b602a29a9ba8ce3fdb48`, recorded the local static-server launch command, and copied the supplied `prog.txt` byte-for-byte into the Rust package's draft `tests/test-cases/testdata/inputs/` fixtures. In a real local browser, verified startup, Start/Pause/Reset, speed/scaling/granularity/audio control cycles, GPIB default devices plus add/remove and duplicate-address behavior, tape import/save/select, the 12-point vector program, Auto Load with and without a tape file, and storage loss after page reload. The no-file Auto Load screen reports `NO PROGRAM FOUND - MESSAGE NUMBER 59`; the successful vector display metrics are recorded in the draft baseline fixture.

The browser logged the existing malformed select/input warning. At this checkpoint, the inherited web-source package's `npm test` exited 1 with `Error: no test specified`; the newer repository-root workspaces were added later. The user deferred the external test environment-mode choice; the decision table records `mock` provisionally with zero dependency rows. The baseline bundle remains a draft because printer output, physical host-key input, complete display-mode captures, and GPIB byte/EOI transcripts were not verified. No inherited web source or asset was modified.

== 2026-11-12 - Vector display-granularity captures

Reran the supplied `prog.txt` at CPU speed `x100` under the three display-granularity labels, paused the emulator, and recorded raw RGB pixel counts in `tests/test-cases/testdata/expectations/display-granularity.json`. `⊚` measured 58,148 non-black pixels; `⊙` and `⊛` each measured 24,973. The latter two settings match for this vector-only program, so character-size rendering remains unverified. A temporary printer probe produced no observable printer output, and a physical-key probe produced no conclusive visible change; GPIB byte/EOI transcripts remain open. No upstream web source or assets were changed.

== 2026-11-12 - Base-ROM startup loop

Traced the reset path beyond its RAM-clear loop. With default PIA input, 100,000 bounded instructions reach a repeatable loop at `0xC6B4`; the PC and complete CPU register state remain unchanged after twelve additional instructions. Added a machine regression test for reaching and repeating that loop. No unsupported opcode was encountered in the bounded run. PIA/keyboard signaling and CPU parity beyond this path remain open.

== 2026-11-13 - Keyboard and interrupt vertical slice

Added raw Tek-key press/release input, active-low Shift and Control inputs, Caps Lock TTY state, and the keyboard's default control-port values. PIA control-line transitions latch CA1/CA2/CB1/CB2 flags using configured edge polarity; reading a data register clears the flags, and writes to the control registers clear stored flags. Machine stepping now checks board IRQ and NMI sources before executing an instruction, routes U565/U461/U265 to IRQ and U561/U361 to NMI, and preserves the reference's held-source entry behavior. Final parity review removed an initially introduced PIA-enable gate because both JavaScript board checks use raw flag bits.

The deterministic base-ROM test boots to `0xC6B4`, clears the CPU IRQ mask for this focused input trace, asserts the keyboard's periodic refresh tick, and runs the ROM IRQ handler until it records `A` at RAM `0x059C` and key-down state at `0x0597`. It then releases the key and bounds execution until the ROM clears the key-down state. Separate tests check active-low modifier values, Caps Lock, selected rising/falling CA1 edges, PIA status/read-clear behavior, all five board sources and four control-line flags with PIA enables clear, CPU IRQ masking, NMI priority, held-NMI suppression and rearming, and the board's ROM vectors. The keyboard refresh remains an explicit call rather than a timer; host-key translation and a complete boot/input trace are not claimed.

== 2026-11-13 - Display handshake and next boot boundary

Traced the masked startup wait to the missing display-ready input rather than bypassing it by clearing CPU I. Added host-facing display-ready and pending-erase APIs, with reference-compatible output-A edge detection and readiness clearing. Renamed the old "completes startup" regression to accurately identify an interrupt-masked display wait. New focused tests cover DDR versus data writes, rising/falling and unchanged output values, port-B exclusion, unrelated-input preservation, and request retention across a status read until completion.

A bounded real-ROM regression first reaches the existing unready wait, calls only `display_ready()`, and then observes `UnsupportedRomBank` for selector 1 at `0x8800` within 200,000 additional steps. It neither changes CPU registers nor suppresses errors. The bank read occurs before any startup erase request in this trace; the erase handshake is verified separately. This establishes the next dependency on the system overflow ROMs, not normal boot, IRQ readiness, visual output, or feature parity.

== 2026-11-13 - Required system overflow ROM integration

Transferred the original `ROM747` and `ROM748` arrays into a Rust-owned 4,096-byte `overflow_rom.bin`, in chip order, with no source-asset changes. Each chip is 2,048 bytes. The combined SHA-256 is `17f387108e9acc1e2b976bc86335784affee99f5ee4cc2ca9f347270b97c46dc`; FNV-1a-64 is `0x01f9dd9364531af2`. The bytes were compared directly to both source arrays during generation; locked tests pin the image fingerprint.

Mapped selector 1 only across the two reference chip windows. Tests cover every mapped byte under all eight expander-address values, chip-boundary read-only behavior, unchanged base ROM below/above the banked window, switching back to selector 0, and explicit errors for unsupported selectors and the upper unmapped overflow range. The mapping regression failed on the old selector-1 error before implementation, then passed.

Updated the bounded startup regression to require execution in the overflow ROM before observing the next explicit error, selector 2 at `0x8800`. The ROM continues without CPU register overrides, but remains interrupt-masked and has not requested erase. This is system-ROM integration and traced progress, not complete startup or feature parity.

== 2026-11-13 - Explicit ignored-bank compatibility

The compatibility form's recommended default was accepted: zero-byte mapping for the two source-ignored selectors, retaining explicit errors everywhere else. Added focused external tests for every byte in the ignored-bank window under all eight expander-address values, write protection, and CPU word-probe behavior. Both new tests failed against the previous strict mapping, then passed after the scoped change. Existing outside-window/base-ROM and unsupported-bank checks remain active.

The bounded startup regression now requires overflow execution and both ignored-bank probes before the selector-4 error. It changes no CPU registers and claims neither complete boot nor normal keyboard readiness. Both required Rust reviewers reported no findings.

= Validation record

The prior Rust validation completed successfully with:

- `cargo fmt --all -- --check`
- `cargo check --locked`
- `cargo build --locked`
- `cargo test --locked` - 40 passed, 0 failed
- `cargo clippy --locked --all-targets -- -D warnings`
- `git diff --check`

The base ROM SHA-256 was checked with `shasum -a 256`. The design, migration plan, and migration log Typst documents compile. These checks validate the earlier Rust slice only; they do not demonstrate complete ROM boot, complete board behavior, native UI behavior, or feature parity.

For the 2026-11-13 keyboard milestone, `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (46 passed), `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` pass. The newly added ROM test is deterministic and bounded: startup uses 100,000 instructions, while each press/release transition searches at most 4,096 steps for the expected ROM RAM state. Its manual interrupt-mask clear is a test setup detail because the observed base-ROM wait-loop state has I set.

For the display-handshake increment, the focused machine suite passes 16 tests and the complete locked suite passes 48 tests, with no failures or skips and exit code 0. Formatting, locked check/build, strict all-targets Clippy, whitespace checks, and compilation of the updated migration log/plan also pass. Generated Cargo and temporary Typst outputs are Git-ignored; temporary probe sources/binaries and PDFs are removed after validation. The inherited web implementation and program/ROM assets remain unchanged.

For system overflow integration, the focused board suite passes 18 tests; the full locked suite passes 50 tests, with no failures/skips and exit code 0. `cargo fmt --all -- --check`, locked check/build, strict all-targets Clippy, whitespace checks, and updated migration-log/plan Typst compilation pass. The new image and Rust sources are not Git-ignored; temporary probes/PDFs are removed, while Cargo build outputs remain intentionally ignored.

For ignored-bank compatibility, the machine suite passes 20 tests and the full locked suite passes 52 tests, with no failures/skips and exit code 0. Formatting, locked check/build, strict all-targets Clippy, whitespace checks, and updated migration-log/plan Typst compilation pass. Temporary probe sources/binaries and PDFs are removed after validation; generated Cargo outputs remain intentionally Git-ignored.

For selector-4 left-backpack mapping, `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (53 passed), and `cargo clippy --locked --all-targets -- -D warnings` pass. The 21-test machine suite verifies the exact 2 KiB `DERROML` bytes read-only for all expander-address field values, and the startup trace requires the header load to produce `0x4051` before stopping at selector 5. The code and test reviewers identified a missing first-tail-byte assertion and a header-probe assertion that preceded the instruction; both were corrected, then the full suite was rerun. Byte-for-byte comparison with the source array, `git diff --check`, and compilation of the migration log and plan also pass. Temporary Typst PDFs were removed; the ROM asset is Git-visible and Cargo outputs remain intentionally ignored. This is bounded ROM progress, not complete boot.

The Phase 0 reference server returned HTTP 200. Its browser checks produced the expected page/control transitions and measured the vector drawing canvas at 1046×830 with 58,148 non-black/green pixels. These observations are captured in the draft baseline fixtures; they are browser-runtime evidence, not Rust parity tests.

== ISS-017 - Left ROM-expander slot data is mapped

*Status: Selector 6 is implemented; selector 7 is tracked separately in ISS-018.*

The active web `RomExp.js` defines eight left-bank slots and returns one slot by the low three address bits. Each slot contains 8 KiB. Rust extracts the exact slot contents in reference order into `romexp-left.bin`, verifies the aggregate SHA-256 `2f7137f745039e043a004b38e0f814788d8c9951d43f88108b66371859838e40` and FNV-1a-64 `0xa81f808d9880651e`, and maps each image read-only at `0x8800..0xA7FF`. Tests compare all 65,536 mapped bytes and attempt writes at both ends of each slot. At this checkpoint, startup executed selector-6 slot 0, verified its `0x4051` header, and reached unsupported selector 7 at `0x8800` with the CPU interrupt mask clear; selector 7 is now mapped as recorded in ISS-018.

The selector-6 image was independently compared byte-for-byte with the active reference function. At that checkpoint, `cargo test --locked` passed 55 tests (32 CPU + 23 machine), and formatting, locked check/build, strict Clippy, diff checks, Typst compilation, and both Rust reviews passed.

== ISS-018 - Right ROM-expander slot data is mapped

*Status: Selector 7 is implemented; bounded startup reads its first slot.*

The active `RomExpRightBank` function selects `BSXRSLOT1`–`BSXRSLOT8` by the low three address bits. Each returned slot is exactly 8 KiB. Rust stores the eight slots in reference order in `romexp-right.bin` (64 KiB), with SHA-256 `d900890f3c71116bfa2a4430ccc63e7c89383143464253f9c3bc6f439b5ce0c3` and FNV-1a-64 `0xf464189c814ef54e`. Selector 7 maps each selected slot read-only at `0x8800..0xA7FF`; tests compare every mapped byte, verify image size/fingerprint, and attempt writes at both ends of each slot. Bounded startup reads the right slot-0 header (`0x4051`) after the backpack and left-expander header probes, with the CPU interrupt mask clear. The subsequent bounded trace reaches base-ROM keyboard code and handles a press/release with explicit ticks; ISS-020 adds coverage for the host's periodic scheduling. This is not a completed boot.

The new image was byte-for-byte compared against all eight slots returned by the active reference function and is not Git-ignored. The new selector-7 mapping test failed against the previous explicit unsupported-bank behavior, then passed after implementation; 56 Rust tests passed at that checkpoint (32 CPU + 24 machine). The Rust Code Quality Reviewer and Rust Test Quality Reviewer found no issues.

== ISS-019 - Normal startup keyboard input path is verified with explicit ticks

*Status: Startup reaches the base-ROM keyboard routine and processes key transitions with explicit ticks; periodic refresh is implemented in ISS-020.*

The bounded startup regression now continues from the selector-7 slot-0 header through base-ROM code until the observed keyboard routine at `0xC88A`, within a maximum of 2,000,000 instructions. It does not override CPU registers. The test injects an `A` press and a keyboard interrupt tick, observes the ROM IRQ handler at `0xEFB8`, and verifies key-down state at RAM `0x0597` and the key value at `0x059C`. A release plus a second explicit tick clears key-down state while preserving the last key value. The I mask is clear from normal startup. The trace verifies the ROM keyboard path with explicit ticks; automatic scheduling and timed erase completion are covered separately in ISS-020.

The Rust Test Quality Reviewer identified an off-by-one issue in the bounded search; the loop now executes at most 2,000,000 instructions and checks the final PC after the last step. The reviewer confirmed the boundary fix and found no remaining issue. `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (56 passed: 32 CPU + 24 machine at that checkpoint), `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and Typst compilation passed.

== ISS-020 - Host-owned execution and timing adapter

*Status: Implemented in `host_runtime.rs` and connected to the native application. The following paragraph records the original adapter checkpoint.*

`HostRuntime` owns elapsed-time accounting outside `Machine`. `start()` signals initial display readiness; each 10 ms deadline runs the CPU batch before the keyboard interrupt callback, matching the reference's interval registration order. At this checkpoint, a 100 ms deadline was reported to the host for display phosphor updates. When `display_erase_pending()` is observed, the runtime calls `display_ready()` after 500 ms. The speed factor follows the reference formula, with the default `x1` budget of 2,264 instructions per interval. Integration tests cover the 10/100/500 ms boundaries, `x2` instruction throughput, invalid speed factors, and propagated machine-bus errors. Subsequent work added the logical display and connected the native egui update loop.

On a machine execution error, the runtime returns any due display-refresh events, stops, and requires the host to reset the machine and create a new runtime. `cargo fmt --all -- --check`, locked check/build/test (61 passed: 32 CPU + 24 machine + 5 runtime), and strict all-target Clippy pass. Both Rust reviewers report no remaining findings.

== Machine, bus, and PIA modules

The CPU-facing `Machine` coordinator remains in `machine.rs`; the `MemoryBus` implementation, memory map, I/O decode, keyboard bus state, and bus error are in `machine_bus.rs`; PIA identifiers and register behavior are in `pia.rs`. The public `machine::` paths for PIA types, keyboard modifiers, and `MachineBusError` remain available through re-exports. At the extraction checkpoint, 55 Rust tests and strict Clippy passed, and the Rust Code Quality Reviewer found no issues.

= Open work and next step

- Complete the Grade 0 reference corpus: character-size rendering, printer output, physical host keyboard input, and GPIB byte/EOI transcripts. Vector rendering is now captured under all three granularity settings.
- Revisit the direct file-load helper and empty-file import only if the user authorizes changes to inherited web behavior; otherwise preserve the observations as migration constraints.
- Verify the native egui display/input integration and screen-copy Save As interaction; preserve the runtime's 10 ms CPU/keyboard, 100 ms display-refresh, and 500 ms erase-completion behavior. Full option-ROM program coverage remains Phase 4.
- Complete the remaining front-panel/lifecycle controls, then port GPIB, storage, Auto Load, and remaining peripherals.
- Re-run the parity checklist against the web reference and document any intentional divergence before sign-off.
