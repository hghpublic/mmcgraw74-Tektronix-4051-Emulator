# Session history - 2026-11-13

## StarWars browser autoplay

- Requested analogous support for the unchanged StarWars FlashDrive assets. Added a shared-root Playwright workspace under `web/playwright/FlashDrive/StarWars`, three spec files, README, and root `test:starwars` / `test:starwars:ui` commands.
- Imported all 11 assets (seven ASCII programs, three binary vector files, LAST); verified binary SHA-256 integrity. Four generators write complete numeric arrays and exercise their original comparison/readback entries. Both R12 fighter choices reach instructions and accept rank-zero exit; the cockpit scenario exercises Shift+UDK6 (UDK16) instruction recall. TLIST lists 1-124 and RETURN executes the original file 1 generator, not a menu.
- First execution exposed the file 14 RUN 5000 dimension/subscript error 10 at line 5080: duplicate line 5020 replaces DIM. This alternate entry and the separate 4054 Option 30 game are explicit skips. File 12 comparison's fragmentary new-design drawing is preserved and documented. No source patches or full-game parity claims.
- Corrected the harness's initially wrong UDK16 scancode, required the completed cockpit border rather than partial graphics/PAGE flash, and compared the recalled instruction heading with the initial heading to exclude the score prompt.
- Before-change `npm test`: 17 passed / 4 skipped, exit 0. Final `npm test`: 26 passed / 6 skipped, exit 0; StarWars subtotal 9 passed / 2 skipped. New module syntax checks and whitespace checks passed; protected source paths have no diff; root workspace links are shared with no per-suite dependency directories.
- Visually checked final cockpit/TIE graphics, recalled instructions, vector comparison and directory screenshots. PNGs are 3200 x 2400. Screenshots, tape/write traces and workspace-cache validation reports are intentionally Git-ignored.
- npm added one local workspace link, not a new third-party package/version. No browser installation, global/account-setting change, emulator change, or supplied asset change.
- Next: use the supported scenarios as reference evidence; source compatibility fixes, alternate-entry repairs, and complete gameplay validation require separate work. The Rust migration remains at the previously recorded core/Phase 0 status.

## StarWars spec rename

- Renamed `StarWars/tests/directory.spec.mjs` to `StarWars/tests/starwars.spec.mjs` without changing its contents. No explicit old-name references required updating.
- `npm run test:starwars -- starwars.spec.mjs`: 1 passed, exit 0. Confirmed the old path is absent. Generated validation output remains intentionally Git-ignored.

## Reverting the StarWars spec rename

- Restored `StarWars/tests/directory.spec.mjs` from `starwars.spec.mjs` at the user's request, with contents unchanged. Preserved all other autoplay work.
- Verified the restored file's syntax and filename; no browser rerun was needed for this filename-only reversal.

## SimpleDemo interactive walkthrough

- Added a chapter to `SimpleDemo-analysis.typ` explaining manual emulator startup via a loopback Python server, seven-file Storage import, paced FIND/OLD/RUN commands, picture inputs, Instant Art, other program numbers, pause/BREAK/reset, and shutdown.
- Distinguished manual play from Playwright UI, documented BUSY versus I/O readiness and messages 59/36, and retained the physical-keyboard coverage caveat. Checked instructions against the actual BASIC programs and emulator controls.
- `typst compile` and `git diff --check` passed. The temporary PDF was generated in the Git-ignored workspace cache and removed; no PDF is retained. Documentation-only change; browser tests were not rerun and emulator/assets are unchanged.

## Rust port status and next step

- Reviewed the root Cargo manifest, Rust machine implementation, migration plan, and draft Phase 0 baseline to answer the request for current status and continuation priorities.
- Reran `cargo test --locked`: 40 passed, 0 failed. Rust remains a core library with partial CPU/board coverage, base ROM and bounded stepping; no executable, egui display, functional keyboard pipeline, or GPIB/tape stack exists yet.
- Recommended the next bounded milestone: implement PIA keyboard/control signaling and route machine interrupts to the CPU; prove key press/release processing through the real ROM with deterministic reference-matched tests. Then finish Phase 1 trace checks and proceed to the Phase 2 egui/display shell.
- Recent autoplay work strengthened the web oracle, not the Rust implementation. Phase 0 remains unfrozen, with character-size, physical keyboard, printer, and byte/EOI gaps to close before parity sign-off. No Rust implementation changes were made in this status discussion.

## Rust PIA keyboard and interrupt milestone

- Implemented Tek key press/release data, active-low Shift/Control, Caps Lock TTY state, and keyboard PIA input values; added CA1/CA2/CB1/CB2 edge flagging, data-read flag clearing, control-write flag reset, PIA interrupt-enable gating, and U565/U461/U265 IRQ versus U561/U361 NMI routing with held-line entry latches.
- The deterministic base-ROM test starts at the repeatable `0xC6B4` loop, explicitly unmasks IRQ for the test (the current loop state has I set), raises the keyboard refresh interrupt, and asserts the ROM records `A` at RAM `0x059C` and key-down state at `0x0597`. Release processing clears the ROM key-down state within a 4,096-step bound. Keyboard refresh is explicit rather than scheduled by a timer.
- Reviewed `TekKeyboard.js`, `TEKTRONIX4051.js`, and `mc6800.js` as read-only behavior references. Rust PIA-enable-gates NMI output; JavaScript `checkNMI` checks raw flags, so this hardware-oriented difference remains to verify against additional reference/board evidence.
- The required Rust code reviewer identified missing NMI enable gating; fixed it and added disabled-NMI coverage. The test reviewer identified modifier bit independence and falling-edge gaps; expanded assertions and added falling-edge coverage. No findings remain open from those reviews.
- `cargo --locked fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (44 passed), `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` passed. `typst compile dev/docs/tek4051/migration-log.typ target/tek4051-migration-log.pdf` passed; the generated PDF was removed. Cargo artifacts remain under Git-ignored `target/`.
- Next: continue Phase 1 CPU/board trace coverage, investigate the ROM wait-loop interrupt-mask state and NMI parity difference, and add real refresh scheduling later; do not claim complete CPU/egui or feature parity.

### Final reference-parity correction and validation

- Final verification found that both JavaScript board checks use raw PIA flag bits, not enable-gated outputs. Removed the initial hardware-oriented gate for IRQ and NMI to preserve the selected reference policy. Added coverage for every PIA and CA1/CA2/CB1/CB2 flag with enable bits clear, plus CPU-mask deferral.
- Focused Rust code review found no issues. Focused test review requested simultaneous-source priority/held-NMI coverage; added a regression proving NMI wins over IRQ, a held NMI does not re-enter or service IRQ, clearing it permits IRQ, and a new edge rearms NMI.
- Final `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (46 passed, 0 failed), `cargo clippy --locked --all-targets -- -D warnings`, and whitespace checks passed. Updated the migration log/plan and handoff to the final reference-compatible state; compiled the Typst documents and removed temporary PDFs.
- Existing external commits advanced HEAD during the implementation; preserved them and did not create or amend commits. No upstream web files/assets, dependency manifests, or global/account settings were changed.

## Rust display handshake and startup diagnosis

- Continued Phase 1 by decoding the `0xC6B4` ROM loop and comparing DisplayX PIA behavior with the unchanged JavaScript reference. The loop waits for DRBUSY-0 high and VPULSE-1 low at `0x8794`; the web Start action sets input bit `0x04`. The I mask is not the root cause of this wait.
- Added `display_ready()` and `display_erase_pending()`. DisplayX data-selected output-A bit-`0x10` falling edges clear readiness and latch an erase request; host completion restores readiness and clears the request. The reference's 500 ms erase delay remains host-owned, not auto-completed or approximated using instruction counts. Rendering and timed scheduling are not implemented.
- Renamed the old "completes startup" test to accurately describe the masked display wait. Added focused edge/DDR/port/readiness tests and a bounded real-ROM regression that leaves the wait with no CPU register override, then observes the existing `UnsupportedRomBank { selector: 1, address: 0x8800 }` error within 200,000 further steps.
- The initial new ROM test incorrectly expected an erase request before the bank read (15 focused passes, 1 failure, exit 101). Corrected it to the observed sequence: the bank read comes first; erase behavior is tested separately. The temporary diagnostic probe also reported the bank error explicitly rather than suppressing it.
- The reference maps selector 1 to system overflow ROMs U101/U201 (`ROM747`/`ROM748`), not optional cartridges. Updated the migration plan/log and handoff: integrate these required ROMs in Phase 1, then continue boot tracing. Optional cartridge/expander integration remains Phase 4. Normal boot/IRQ readiness is not claimed; the prior focused keyboard-handler test still explicitly unmasks IRQ.
- Required Rust Code Quality Reviewer and Rust Test Quality Reviewer both reported no findings. Focused machine suite: 16 passed. Final `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (48 passed, 0 failed/skipped), `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` passed.
- Both updated Typst documents compiled successfully. Temporary probe source/binary and PDFs were generated under Git-ignored `target/` and removed; Cargo build output remains intentionally ignored. Implementation and documentation changes are not ignored. No upstream files/assets, manifests, dependencies, global/account settings, or commits were changed.
- Next: port and verify the required system overflow ROM mapping, then seek normal ROM boot and keyboard readiness without test-only CPU register changes.

## Required system overflow ROMs

- Continued with U101/U201 (`ROM747`/`ROM748`). Generated a Rust-owned 4,096-byte immutable image from the two original 2,048-byte arrays, verified byte equality during generation, and pinned SHA-256 `17f387108e9acc1e2b976bc86335784affee99f5ee4cc2ca9f347270b97c46dc` and FNV-1a-64 `0x01f9dd9364531af2`. Original web assets remain unchanged.
- Mapped selector 1 at `0x8800..0x97FF`, independently of the expander-address field. Kept selector 0 and base-ROM reads outside the bank window unchanged; selectors 2-7 and selector 1's upper unmapped window still return explicit errors.
- The new mapping regression failed on the old selector-1 error before implementation, then passed. Tests verify every overflow byte across all eight expander addresses, both chip boundaries/read-only writes, switch-back, unsupported ranges, and base-ROM preservation under every nonzero selector.
- Bounded startup now executes overflow-ROM code without CPU register overrides, then stops at `UnsupportedRomBank { selector: 2, address: 0x8800 }`. A ready-from-reset probe completed 102,553 instructions before `LDX` at `0xAA8F` hit that boundary; no IRQ enable or erase request occurred first. The web reference ignores banks 2/3 and its word reader coerces their `undefined` values to zero. No blanket zero/base-ROM fallback was added.
- Required code review reported no findings; its isolated rustc attempt was denied filesystem access, while repository-root Cargo validation succeeded in the main session. Test review identified missing base-ROM boundary assertions with unsupported selectors active; added them for selectors 1-7 and reran validation.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (50 passed, 0 failed/skipped), `cargo clippy --locked --all-targets -- -D warnings`, and whitespace checks passed. Focused board suite: 18 passed. Updated migration log/plan compiled successfully with Typst.
- Updated the handoff and migration documentation to record completed system-ROM integration and the next empty-bank probe gap. Temporary probe source/binary and PDFs were removed from Git-ignored `target/`; Cargo artifacts remain intentionally ignored. The new ROM and Rust/documentation changes are not ignored.
- Existing external staging/commit activity advanced HEAD from `76b95bc` to `121b75e` during the work, incorporating the implementation. Preserved that activity; issued no git add/commit/amend commands. No dependency, upstream source/asset, or global/account-setting changes.
- Next: model and verify reference-compatible empty-bank startup probing, then continue boot tracing without CPU register overrides; normal boot, timed refresh/erase, display rendering, and egui remain incomplete.

## Separating public and private Rust tests

- Requested public-API tests in a separate test directory and private-API tests in private modules. Classified all 50 existing tests: 32 CPU and 18 machine/ROM tests exercise public APIs; no direct private-production-function/state tests exist. References to private architectural constants were test fixtures, not private-API tests.
- Moved the suites into `dev/src/4051-Emulator/rs/4051-Emulator-rs/tests/cpu.rs` and `tests/machine.rs`, importing the library as an external consumer. Registered both nonstandard paths with root Cargo `[[test]]` targets. Kept fixture constants local to tests; did not widen production visibility or invent empty private test modules.
- Mechanically compared against baseline `97b45a0`: all 50 test names/bodies and the CPU bus fixture helpers are preserved, apart from imports, namespace qualification, indentation, and formatting. Both production prefixes are unchanged. Existing baseline fixtures remain untouched.
- Documented public integration tests versus future private `#[cfg(test)] mod tests` modules in the Rust setup guide and handoff. Focused runs now use `cargo test --locked --test cpu --test machine`; old inline-module name filters no longer select these suites.
- Focused integration validation passed 32 CPU + 18 machine tests. `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (50 passed, 0 failed/skipped; 0 remaining library unit tests), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- Required Rust Test Quality Review reported no findings and independently verified discovery and focused targets. Whitespace checks passed. New tests are not Git-ignored; normal Cargo output remains intentionally ignored. External commit activity incorporated the relocation during this task; preserved it without issuing staging/commit commands. No emulator behavior, dependencies, upstream assets, or global/account settings changed.
- Next emulator work remains the selector-2 empty-bank startup probe; this request only reorganized tests and documented their public/private boundaries.

## ROM asset directory and filename cleanup

- Moved the Rust-owned `base_rom.bin` and `overflow_rom.bin` assets into `roms/base.bin` and `roms/overflow.bin`. Updated `rom.rs` include paths, current handoff links, and migration-log location guidance; preserved historical records of the former filenames.
- Verified both SHA-256 digests are unchanged and the old paths are absent. Public ROM constants, bytes, emulator behavior, and upstream assets are unchanged.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked --test machine` (18 passed, 0 failed/skipped), and strict all-targets Clippy passed. Required Rust code review reported no findings. The updated migration log compiled with Typst.
- Destination ROMs are not Git-ignored. Cargo outputs and the temporary PDF are intentionally ignored; removed the PDF after validation. External commit activity incorporated the move during review; preserved it without issuing staging/commit commands. Next emulator work remains empty-bank startup probing.

## Cargo workspace and member package

- Discussed the layout without editing files, confirmed the repository root should own the workspace, then implemented it after the user's approval.
- Converted root `Cargo.toml` to a virtual workspace with resolver 3 and the single member `dev/src/4051-Emulator/rs/4051-Emulator-rs`. Added the member's package manifest, preserving package metadata and pointing `[lib]` at `lib.rs`. Removed explicit root test targets; Cargo now discovers the existing member integration suites automatically.
- Root `cargo metadata --locked --no-deps --format-version 1` confirms one member/default member, the expected library and both test targets, and root `target/`. Member-directory `cargo locate-project --workspace` resolves the root manifest; its test inventory contains the same 32 CPU + 18 machine tests. The root lockfile is unchanged, with no member-local lockfile or target directory.
- Root `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (50 passed, 0 failed/skipped), and strict all-targets Clippy passed. Both updated migration Typst documents compiled successfully; temporary PDFs were removed.
- Updated the setup guide, migration plan/log, handoff, and outdated test-layout memory. The member manifest is not Git-ignored; shared Cargo outputs remain intentionally ignored. Preserved existing history edits and external staging without staging/commit commands. No Rust implementation/tests, ROM bytes, dependencies, upstream assets, or global/account settings changed.
- Next emulator work remains reference-compatible empty-bank startup probing.

## Explicit ignored-bank compatibility and the next startup boundary

- Continued the Rust port from the selector-2 startup probe. Presented a policy form with zero-byte ignored-bank mapping as its recommended default. The form was accepted without returned field values; proceeded using that displayed default, not an explicit returned selection.
- Selectors 2/3 now read zero only within `0x8800..0xA7FF`, independently of the expander-address field. This deliberately normalizes JavaScript absent byte values into Rust bytes while preserving the reference word-probe result; it is not floating-bus emulation or complete undefined/NaN byte-arithmetic parity. Other unsupported mappings remain explicit errors.
- Added public-API regressions for every byte in the ignored-bank window under all eight expander-address values, read-only writes, and CPU word probes. Both new tests failed against the previous strict mapping before passing with the scoped change. Existing outside-window/base-ROM and unsupported-selector assertions remain active.
- The real-ROM regression now requires overflow execution and both ignored-bank probes, then stops at selector 4 (`DERROML`, the default left backpack), address `0x8800`, with the failing `LDX` at `0xAA8F`. A ready-from-reset diagnostic completes 102,599 instructions before this error. No CPU registers are overridden; no erase request or IRQ enable occurs first. Complete boot remains unverified.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (52 passed: 32 CPU + 20 machine; 0 failed/skipped), and `cargo clippy --locked --all-targets -- -D warnings` passed. Both required Rust reviewers reported no findings and independently passed the 20-test machine suite.
- Updated the migration log/plan and handoff. Both Typst documents compiled successfully; whitespace checks passed. Source/tests/docs are Git-tracked, not ignored. The diagnostic source/binary and temporary PDFs were Git-ignored and removed after validation; normal Cargo output remains intentionally ignored. Preserved external staging/commits without issuing staging/commit commands. No upstream source/assets, dependencies, or global/account settings changed.
- Next: integrate and verify the reference's actual boot-visible backpack mapping beginning with selector 4, then continue bounded startup tracing. Do not mask missing ROM data by broadening the ignored-bank zero policy.

## CPU code and test comments

- Ran the Rust implementation-comment and test-comment skills for the tagged CPU file and its directly related public CPU tests only.
- Expanded public API docs in `cpu.rs` to describe the staged surface, register/opcode observation, PC outcomes, bus-error progress, reset failure atomicity, and IRQ/NMI mask semantics. Kept the two existing reference-quirk comments.
- Added only targeted test comments for interrupt entry order, signed/wrapping branch offsets, and stack state after bus failures; test behavior and assertions are unchanged.
- `cargo fmt --all -- --check`, `cargo test --locked -p tek4051-emulator --test cpu` (32 passed), strict-warning `cargo doc --locked -p tek4051-emulator --no-deps`, and `git diff --check` passed. `target/doc/` generated documentation is Git-ignored; changed Rust files remain Git-visible.
- Next emulator implementation dependency remains the reference's selector-4 left backpack ROM; this comment-only pass does not change it.

## Expanded CPU design and test comments

- Responded to the user's comment-level requirements by expanding the `cpu.rs` design explanation: the bus boundary, opcode dispatch stages, addressing/decode families, stack and interrupt frames, word byte order, partial bus-error progress, branch conditions, and arithmetic flags are described near the relevant logical groups and public API.
- Added an explanatory module overview to the public CPU test suite. Each of its 32 tests now begins with What/Why comments and includes context-specific Arrange, Act, and Assert phase notes; multi-stage tests explain the important transitions without changing their assertions.
- Audited all 32 test functions for the required sections and checked comment lines for readability. `cargo fmt --all -- --check`, `git diff --check`, `cargo test --locked` (52 passed, 0 failed), and `RUSTDOCFLAGS='-D warnings' cargo doc --locked -p tek4051-emulator --no-deps` passed.
- Generated rustdoc at `target/doc/`, which is Git-ignored; Rust source, tests, handoff, and history are Git-visible. No behavior or emulator data changed. Next emulator work remains the selector-4 backpack mapping.

## CPU integration-test and test-comment review

- Reviewed all 32 cases in `tests/cpu.rs` as public-API tests. They provide focused coverage for arithmetic/condition codes, branches, addressing modes, stack/control flow, interrupts, and bus-error state; no current test defect or vacuous assertion was found.
- Recorded follow-up coverage opportunities, not blockers: word operations through direct/indexed modes, RTI pop failures, and partial failures in word stores. The suite remains a staged-CPU test set rather than broad opcode-complete certification.
- Reviewed all five test comments: they are sparse and add rationale for otherwise subtle transitions. Clarified the BSR and RTS comments so they accurately describe both injected failure cases. Test logic is unchanged.
- `cargo fmt --all -- --check`, `git diff --check`, and `cargo test --locked -p tek4051-emulator --test cpu` passed (32 tests, 0 failures).

## Evidence-grounded Rust comment requirements

- Reviewed the Rust code/test comment skills, workspace instructions, and Rust setup guide after the user clarified that comments must be grounded in code analysis and that the required What/Why and Arrange/Act/Assert explanations are obligatory for every test.
- Updated workspace instructions and both skills to require verified claims, human-readable implementation overviews and significant logical-group explanations, and substantive What/Why plus Arrange/Act/Assert comments in each test. Updated the Rust setup guide to defer explicitly to those repository-specific requirements, avoiding conflict with generic skill advice.
- Updated the handoff to note that the existing CPU comments have not yet been re-audited against the clarified standard. No Rust implementation or test comments were changed in this configuration task.
- Reviewed the general Copilot setup guide: its instruction to follow project-specific requirements is consistent and needed no change. `git diff --check` and targeted instruction consistency review passed; no runtime tests were applicable to this documentation/configuration-only change.

## Rust reviewer quality gates

- Expanded the Rust code-quality reviewer to treat readability, maintainability, testability, and modularity as core requirements and explicitly assess cohesive, human-comprehensible functions, consistent abstraction levels, relevant extension boundaries, and evidence-based findings.
- Expanded the Rust test-quality reviewer to assess test readability, setup/helper complexity, fixture responsibilities, testability at stable behavior boundaries, and maintainable extension of relevant coverage without requiring speculative architecture.
- No comment skills, Rust source, or tests were changed in this request. Validation: `git diff --check` and focused review of the reviewer instructions.

## Copilot config consistency and requirement conflicts

- Added workspace and setup-guide requirements to review agent/skill configs alongside related instructions for clarity, scope, workflow, constraints, outputs, and consistency; resolve contradictions rather than layering conflicting rules.
- Added a requirement to explicitly identify conflicts among user requirements or with applicable repository guidance; clarify precedence when it is unclear and state the resolution when the user's intent is explicit.
- Updated the handoff. No Rust implementation, tests, agents, or skills changed in this request; `git diff --check` and targeted config consistency review passed.

## Idiomatic Rust test review

- Updated the Rust Test Quality Reviewer to explicitly assess idiomatic, readable Rust test code, including test structure/names, assertions, error handling, types/ownership, fixtures, and helpers. It is instructed not to impose one assertion per test or a universal fixture pattern.
- Updated the handoff. No Rust source or tests changed; validation is limited to instruction consistency and `git diff --check`.

## Selector-4 left-backpack ROM mapping

- Extracted the exact 2,048-byte `DERROML` array from the unchanged web ROM source into the Rust-owned `roms/backpack-left.bin`. Verified byte-for-byte identity; SHA-256 is `cb16cccbf9f9c45ed2e655e786642c07ebf253caff407912f574af145d59624f`.
- Selector 4 now maps that read-only payload at `0x8800..0x8FFF` independent of the expander-address field. The web selects the array over an 8 KiB window although the array contains only 2 KiB; the Rust bus returns an explicit unsupported-bank error for the uncovered tail instead of inventing bytes.
- Added fingerprint and full-byte mapping coverage for all eight expander-address values. The real-ROM regression now verifies `LDX $8800` loads header `0x4051`, then stops at selector 5 / `0x8800`; no CPU register override, IRQ enable, erase request, or complete boot is claimed.
- Code and test reviewers identified the test's missing `0x9000` tail-boundary check and that the startup test marked the header probe before stepping it. Added the boundary and asserted X equals `0x4051` after the instruction; reran the machine suite and full workspace validation.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (53 passed: 32 CPU + 21 machine), `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and both Typst document compilations passed. Removed the named temporary PDFs. The new ROM is not Git-ignored; normal Cargo output under `target/` remains intentionally ignored.
- Next: inspect and integrate selector 5 (`DERROMR`) from its actual source bytes, keeping unsupported data explicit, then continue bounded startup tracing.

## Reviewing named hardware values

- Reviewed the Rust CPU and machine code, with the web reference used to ground hardware terminology. CPU register fields, reset/IRQ/SWI/NMI vectors, condition-code flags, RAM/ROM/I/O regions, PIA bases, and the bank-select register already have semantic names.
- The machine's bank decode still embeds `0x8800`, `0x9800`, and `0xA800` in range/offset logic; `0x9000` is the ROM747/ROM748 split in the reference and is used by tests, while Rust stores those chips in one concatenated image. The reference identifies the bank-switch window and ROM747/ROM748, DERROML/DERROMR regions; these boundaries are stronger naming candidates than instruction opcodes or test fixture bytes.
- PIA register offsets and control-line masks also remain numeric, but naming them requires preserving the PIA semantics visible in the current logic and tests. The public-API tests' duplicated reset-vector and machine-address constants remain independent expected values; do not expose private implementation constants just to deduplicate tests.
- No source/test changes or identifier spellings were proposed. Next: agree on the reference-grounded naming vocabulary before changing code, then continue the selector-5 migration.

## Applying named hardware values

- Following the review, added named bank-switch start/end and overflow-ROM end constants, named the PIA port/control offsets and the PIA/display masks whose behavior is explicit in the machine code. Names use existing project/reference terms such as bank switch, overflow ROM, PIA port/control lines, display ready, and erase request.
- Kept CPU register fields, vector/CCR identifiers, opcode literals, and public test expectations unchanged; no tests were edited or coupled to the private machine constants.
- `cargo test --locked` passed all 53 tests. `cargo fmt --all` corrected formatting, then `cargo fmt --all -- --check`, strict all-target Clippy, and `git diff --check` passed. The Rust Code Quality Reviewer found no issues.

## Selector-5 right-backpack ROM mapping

- Extracted the exact `DERROMR` array from the unchanged web source to `roms/backpack-right.bin`. It contains 8,192 bytes, all `0xFF`; byte-for-byte comparison passed. SHA-256: `7d2c7ac4888bfd75cd5f56e8d61f69595121183afc81556c876732fd3782c62f`; FNV-1a-64: `0x9c50825ef0adc325`.
- Selector 5 now maps the full image read-only at `0x8800..0xA7FF`, independent of the expander-address field. Added size/fingerprint and exhaustive per-address-field mapping assertions, plus writes at both ends of the window. Updated the real-ROM trace to confirm the right-backpack header word is `0xFFFF` and stop at selector 6 / `0x8800`.
- The new mapping test failed before implementation on the explicit selector-5 error, then passed after mapping. Full validation passed: `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (54 passed: 32 CPU + 22 machine), `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and both Typst compilations. Temporary PDFs were removed; `backpack-right.bin` is not ignored, while Cargo output under `target/` remains intentionally ignored.
- The Rust Code Quality Reviewer found no implementation issues. The Test Quality Reviewer reported the test file was already in HEAD and therefore outside its diff scope; I directly reviewed the selector-5 assertions and the full suite passes.
- Next: inspect selector 6 (`RomExpLeftBank`) source bytes and mapping behavior before integration; keep unsupported data explicit.

## Selector-6 left ROM-expander mapping

- Inspected the active, unchanged `RomExp.js`: `RomExpLeftBank` maps its low three-bit slot argument to `BSXLSLOT1` through `BSXLSLOT8`. Each returned slot is 8 KiB. Extracted the eight arrays in slot order into Rust-owned `roms/romexp-left.bin` (64 KiB), then independently compared every extracted byte against the reference function. SHA-256: `2f7137f745039e043a004b38e0f814788d8c9951d43f88108b66371859838e40`; FNV-1a-64: `0xa81f808d9880651e`.
- Implemented selector 6 as read-only mapping for all eight slots over `0x8800..0xA7FF`, using the bank register's address field. Added image length/fingerprint coverage, byte-for-byte mapping over all 65,536 bytes, slot-boundary write checks, and unsupported-window assertions.
- The new mapping test failed before implementation at selector 6 / `0x8800`, then passed after mapping. Bounded normal startup now verifies the selector-6 slot-0 header (`0x4051`), reaches unsupported selector 7 at `0x8800`, and has cleared the CPU interrupt mask. This does not establish timed keyboard refresh or end-to-end keyboard IRQ readiness.
- Validation passed: `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (55 passed: 32 CPU + 23 machine), `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and compilation of both updated Typst documents. The Rust Code Quality Reviewer and Rust Test Quality Reviewer found no issues. The new ROM is not Git-ignored; Cargo outputs in `target/` remain ignored. No inherited web files were changed.
- Next: inspect selector 7 (`RomExpRightBank`) and its exact right-bank slot payloads in the active reference before mapping; keep absent data explicit.

## Extracting PIA and machine-bus modules

- Moved the `MachineBus` memory map, `MemoryBus` implementation, I/O decode, keyboard bus state, and bus error to `machine_bus.rs`. Moved PIA identifiers/register state and read/write/control-line behavior to `pia.rs`; `machine.rs` now coordinates CPU execution and the public machine API.
- Preserved existing `machine::` paths for `PiaId`, `PiaPort`, `PiaControlLine`, `KeyboardModifier`, and `MachineBusError` through re-exports. Added the public `pia` module while keeping `machine_bus` internal.
- No emulator behavior or tests changed. `cargo check --locked`, `cargo test --locked` (55 passed: 32 CPU + 23 machine), and strict all-target Clippy passed after the extraction. The Rust Code Quality Reviewer found no issues; final formatting/build/diff and Typst checks pass.
- Next: keep selector 7 (`RomExpRightBank`) as the next ROM boundary; inspect its source bytes before implementing.

## Selector-7 right ROM-expander mapping

- Inspected the active, unchanged `RomExp.js`: `RomExpRightBank` selects `BSXRSLOT1`–`BSXRSLOT8` by the low three address bits. Each slot returns exactly 8 KiB. Extracted the eight slots in reference order into Rust-owned `roms/romexp-right.bin` (64 KiB), then compared every byte against the reference function. SHA-256: `d900890f3c71116bfa2a4430ccc63e7c89383143464253f9c3bc6f439b5ce0c3`; FNV-1a-64: `0xf464189c814ef54e`. The asset is not Git-ignored; no inherited web files were changed.
- Implemented selector 7 as read-only mapping for all eight slots over `0x8800..0xA7FF`, using the bank register's address field. Added image length/fingerprint coverage, byte-for-byte mapping of all 65,536 bytes, writes at both ends of each slot, and unsupported-window assertions.
- The new mapping test failed before implementation at selector 7 / `0x8800`, then passed after mapping. Bounded normal startup verifies the selector-7 slot-0 header (`0x4051`) after both backpack and left-expander probes; the CPU interrupt mask is clear. A separate 2,000,000-instruction trace reaches base-ROM address `0xF382` without a bus error, but this does not establish a completed boot or keyboard/input readiness.
- The Test Quality Reviewer identified that the startup trace marked ignored-bank probes without asserting their resulting header value. The test now asserts `IX == 0` after each selector-2/3 probe before marking it observed; the focused startup test passes. The reviewer confirmed the gap is resolved with no remaining issue in that scope.
- Rust validation passed: `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (56 passed: 32 CPU + 24 machine), `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and Typst compilation of the migration log. The Rust Code Quality Reviewer found no issues. Timed keyboard refresh and end-to-end keyboard IRQ readiness remain unverified.
- Next: continue bounded normal-startup tracing beyond the selector-7 slot-0 probe without CPU register overrides or broadening the ignored-bank zero policy; investigate subsequent execution only from observed ROM bytes and explicit host/timing behavior.
