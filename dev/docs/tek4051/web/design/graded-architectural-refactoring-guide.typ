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
  #text(size: 15pt)[Graded Architectural Refactoring Guide]
]

#align(center)[
  Track: architecture and project structure, independent of migration
  \
  Design basis: `design.typ`, agreed boundary discussion of 15 November 2026
  \
  Implementation root: `dev/src/4051-Emulator/rs/4051-Emulator-rs`
  \
  Status: AR-0 through AR-2 evidence recorded; AR-3/AR-4 implementation evidence exists; integrated AR-3 through AR-5 acceptance remains open
  \
  Updated: 17 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and independent tracks

This guide sequences behavior-preserving architectural refactoring of the existing Rust emulator. It does not sequence the remaining web-to-Rust migration. The agreed responsibility boundaries and architecture diagram remain in `design.typ`; this guide supplies a separate progression and evidence requirements for reaching them. Creating this guide does not authorize implementation or settle every interface and packaging decision.

Use the prefix *AR* for architectural grades. They are ordered acceptance gates, not percentages or quality scores. A grade is complete only when its work and exit evidence have been reviewed. Later work does not waive an earlier gate.

#table(
  columns: (1fr, 2fr, 2fr),
  stroke: 0.5pt,
  [*Track*], [*Success means*], [*Does not establish*],
  [Architectural refactoring: AR-0 to AR-5], [Existing Rust behavior is retained behind clear, testable ownership and dependency boundaries.], [New emulator capabilities, complete hardware behavior or web feature parity.],
  [Migration: existing grades 0 to 5], [The agreed web behavior and supplied workflows are reproduced with recorded parity evidence.], [Completion of architectural refactoring merely because a feature works.],
)

`graded-migration-guide.typ` and `migration-plan.typ` retain responsibility for migration scope and parity. A frozen, complete web baseline is not a prerequisite for starting this refactoring track: its preservation baseline is the current Rust implementation and its established contracts. The unchanged web reference remains corroborating compatibility evidence, not permission to repair unrelated behavior during a move.

Keep the tracks in separate scoped changes and acceptance records. If a migration feature genuinely requires a boundary change, record that specific dependency and the minimum architectural work needed. Neither track automatically advances the other.

== Scope and non-goals

In scope are ownership, module organization, dependency direction, host/machine input separation, application I/O adapters and preservation of the nonblocking GUI boundary.

Out of scope are additional CPU instructions, peripherals, BASIC compatibility fixes, new storage features, altered persistence, new rendering algorithms, speed calibration, and changes to supplied ROM/program assets. The unresolved German Option+L mapping is a separate behavior issue; isolating its translation code does not authorize changing its mapping. Record newly discovered defects separately unless a regression was caused by, or is inseparable from, the current refactoring increment.

The current worker architecture is the starting point, not work to discard. Preserve its exclusive mutable machine ownership, bounded nonblocking communication, immutable publication, explicit failures, acknowledged modal/readiness changes and shutdown outside the GUI event loop.

= Boundary contract and starting assessment

The target responsibility direction is:

```text
application
    GUI -> emulation -> machine
```

- *Machine:* CPU, bus/memory, PIAs, ROM mapping and images under `machine/roms/`, tube/display behavior, machine keyboard signals and machine-level device I/O. It must not depend on either application layer.
- *Application emulation:* machine ownership, deterministic execution coordination, elapsed-time runtime, lifecycle and speed policy, worker transport, framework-neutral input sequencing and program launch. It must be usable without GUI framework types or a window.
- *Application GUI:* widgets, host keyboard/pointer interpretation, focus and shortcut policy, front-panel presentation and view-specific image preparation.
- *Host adapters:* operating-system clock, dialogs, file operations and native startup/worker integration. Their work may execute on background threads without becoming machine behavior.
- *Shared Flash Drive codecs:* machine-independent metadata and archive interchange formats. Keep tape storage/transfer semantics in the machine, filesystem access in host adapters and directory presentation in the GUI.

The distinction is about responsibility, not thread identity. Expensive display conversion and host I/O must remain off the repaint path after being assigned to their appropriate application components.

At the AR-0 baseline, the library exposed flat CPU, display, GPIB, timing, machine, PIA and ROM modules. AR-1 groups hardware under `machine`. The initial source-grouping increment now places the session, neutral input sequencing and elapsed-time runtime together under `emulation/`; root re-exports preserve the public `emulation_input` and `host_runtime` paths. Shared metadata and archive codecs now live under `flash_drive/`. Native presentation, panel widgets and event translation now live physically under `gui/`; `native/app.rs` preserves their sibling module scope through `#[path]`. Native startup, its worker/image bridge, and native private fixtures now share `native/` while preserving module names. The library owns neutral contracts and reusable session execution. Native consumers use explicit session operations rather than mutable machine/runtime/input fields; `host/io.rs` accesses archive streams through that owner. The host-clock implementation is physically at `host/clock.rs`, while `lib.rs` preserves its public root `host_clock` path and `Machine::default()` compatibility binding. Private native fixtures are descendants only in native-feature test builds. The default-enabled `native` feature permits a package-selected GUI-free library build; the opt-in `wasm` feature adds a browser-facing `wasm-bindgen` session API using browser-compatible time/RTC support. That API shares session behavior but leaves key mapping and rendering to its browser caller. Broader crate/package decisions remain open.

Further physical moves and any separate Cargo crates remain decisions to make during this track. Prefer the smallest enforceable organization. A separate machine crate needs immediate value such as dependency enforcement or avoiding GUI compilation for machine builds. A crate per device, generic plugin architecture or another object forwarding every machine operation is not required.

== Proposed source grouping and cost

The agreed directory direction is `machine/`, `emulation/`, `host/`, `gui/`, `native/` and a shared `flash_drive/` domain group. The emulation move, shared Flash Drive codec grouping, and physical host, GUI and native groupings are implemented. Preserve existing behavior and public module paths while changing declarations and imports.

- The session, neutral input sequencing and elapsed-time runtime now share `emulation/`. Root re-exports preserve the existing `host_runtime` and `emulation_input` public paths without a wrapper object or duplicate state.
- `emulation::SPEED_FACTORS` is the single supported multiplier list used by session speed-index validation and GUI labels/cycling. The native binary consumes the library-owned policy rather than maintaining a second array.
- Shared metadata, JSON, filename and archive codecs now live under `flash_drive/`. GPIB tape behavior, host filesystem operations, and GUI controls remain in their respective areas.
- The clock implementation, dialogs, selected-file reads and effect writes now live physically under `host/`. The public `host_clock` path remains at the crate root and `Machine::default()` retains its existing callback. Native dialog/import/effect declarations remain siblings under `native/app.rs` to preserve private visibility. The GUI app, panel, keyboard adapters and their private test files now live physically under `gui/`, also retaining their sibling module scope. `native/app.rs`, its backend, and private native fixtures share `native/` without changing the sibling module relationships. Consider future startup and worker changes separately and only where dependency direction permits.
- `native/backend.rs` is an explicit bridge combining worker transport with egui image preparation. Keep that cross-boundary role clear, and split it only if a concrete dependency benefit outweighs added machinery.

The initial module moves and public re-exports have no runtime performance cost: a re-export is a compile-time path alias, not a runtime forwarding call. The design must not add copies, allocations, dynamic dispatch, channels, or extra work to instruction, framebuffer, worker, or file-I/O paths as a side effect of folder symmetry. A separate crate could change compile/dependency costs, but is not part of this directory reorganization and still requires a demonstrated enforcement or GUI-free build benefit.

The main complexity is at compile time and in maintenance: module declarations and consumers must be rewired; private tests may rely on descendant visibility; and existing public paths need compatibility coverage. Move one responsibility group at a time, compile the relevant consumers, and run existing behavior tests rather than adding tests that only assert file placement.

The RTC default is a compatibility constraint, not an ordinary file move. `machine/gpib.rs` still uses `crate::host_clock::local_datetime` for `Machine::default()`. The implementation now lives in `host/clock.rs`, and `lib.rs` explicitly preserves that public compatibility path with `#[path]`; constructor behavior and the function-pointer seam are unchanged. Removing the host-bound default remains a separate behavior/API decision.

== Grade overview

#table(
  columns: (0.6fr, 1.8fr, 2.6fr),
  stroke: 0.5pt,
  [*Grade*], [*Boundary gained*], [*Primary exit evidence*],
  [AR-0], [Preservation contract and scope], [Recorded starting state, reproducible checks and agreed ownership decisions.],
  [AR-1], [Cohesive machine area], [Machine components grouped with deliberate visibility and unchanged observable behavior.],
  [AR-2], [Host versus machine input], [Framework input ends at an application adapter; neutral input preserves machine signals and timing.],
  [AR-3], [GUI-independent emulation], [A headless session and neutral command/snapshot contract preserve worker and responsiveness guarantees.],
  [AR-4], [Application I/O boundary], [Host effects are application-owned; device semantics and acknowledged operation ordering are unchanged.],
  [AR-5], [Verified integrated structure], [Dependency review, regression evidence, focused native checks and both required Rust reviews.],
)

Writing this guide alone did not certify any grade. The records supply AR-0/AR-1 and AR-2 technical exit evidence, followed by reusable-session, GUI-free-build, host-effect, codec, and source-grouping evidence for AR-3/AR-4. As of 17 November, no integrated AR-3 or AR-4 exit decision is recorded, and AR-5 remains open. AR-0 precedes implementation; AR-1 through AR-5 are the acceptance order. Small independent preparations may overlap only after their interfaces are agreed and without claiming a later grade has passed.

== Current status - 17 November 2026

No AR grade is recorded as formally accepted. The preservation baseline and machine/input boundary have implementation and test evidence; reusable session execution and a package-scoped GUI-free build have also been demonstrated. Host adapters, shared Flash Drive codecs, and source groupings have progressed, but remaining host-I/O ownership/ordering concerns and the absence of an integrated boundary review keep AR-3/AR-4 acceptance open. AR-5 still requires the integrated dependency/ownership review, focused native checks, and both required Rust reviews. Physical native acceptance remains unverified. These architectural gates remain independent of migration grades 0-5.

== First increment record - 15 November 2026

The user subsequently authorized starting refactoring. The bounded first increment establishes the Rust preservation baseline and groups existing machine code; it does not change input, scheduling, device behavior or migration criteria.

*Baseline and scope:* revision `a0bf08563fefd70ce656b6d9c4889a5ac28d113c`, with existing changes to design, migration/refactoring guides and chat records, but no Rust changes. The formatter, default tests for both Rust packages, strict emulator Clippy and locked build passed before moving files. Native macOS interaction was not revalidated.

*Boundary gained:* `machine/mod.rs` owns the existing `Machine` API and declares CPU, display, GPIB, PIA and ROM component modules. Its bus is private in `machine/bus.rs`; machine-internal imports stay within that hierarchy. Host timing remains in `host_runtime.rs`; native input, GUI and worker code remain application-side. Both packages' consumers now use `tek4051_emulator::machine` paths, without duplicate crate-root aliases. No crate split, dependency or forwarding abstraction was introduced. ROM binaries remain unchanged at their existing `roms/` paths; only compile-time include paths changed.

*Preservation evidence:* the same 239 emulator tests and 13 shared tests (one keyboard, seven session and five worker) pass before and after the move. The private GPIB/tape tests moved without assertion changes; public tests remain in package `tests/`. Existing x100 and gated host-export tests retain their strict sub-250 ms headless repaint bound in the debug test build. This is not a physical-window latency claim.

The following post-change commands passed:

```sh
cargo test --locked -p tek4051-emulator \
  -p tek4051-flash-drive-tests --quiet
cargo clippy --locked --workspace \
  --all-targets --all-features -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

Both required Rust reviewers reported no actionable findings, including verification of moved-file identity, test discovery, private-bus visibility, ROM paths and consumers. All interactive targets compile; no graphical test or emulator window was launched. AR-0's baseline and AR-1's technical exit evidence are recorded, not acceptance of AR-2 through AR-5 or migration parity. Host-clock sampling and interchange codecs remain explicit AR-4 work.

== Second increment record - AR-2 neutral input

*Baseline and scope:* revision `acc5c42a4c55c7727b38c5bb01d637d93b524d91`, with existing handoff/history changes preserved. All 92 native binary tests passed before extraction. The user authorized continuing architectural refactoring, not changing key mappings or migration behavior.

*Boundary gained:* `emulation_input.rs` provides concrete `KeyModifiers`, `KeyboardState` and `InputSequence` types. Host adapters supply Tek codes, Shift/Control values and press/release/cancel operations; no egui key, event, context or rectangle enters this module. Application source ownership is distinct from the actual keyboard/PIA state, which stays in the machine. The sequencer retains 100 ms press/release phases, manual replacement, macro busy checks, fixed-modifier scripts and error-tail erase advancement.

`keyboard_input.rs` keeps text/key pairing, native identity and ownership, focus/editing policy and pointer interpretation. `front_panel.rs` contains only widgets and clipped hit regions. Production emulation owns the neutral sequencer separately from those adapters. An explicit private ownership enum prevents a stale host release from releasing a newer panel key; no event bus, trait framework, dependency or crate was added.

*Evidence:* two new public-API tests under `tests/emulation_input.rs` exercise real PIA key/modifier values across 99/100 ms boundaries, queued taps, busy rejection, source replacement, late releases and cancellation without constructing a GUI context. Existing adapter/widget, complete macro, real-ROM, transport, x100 and blocked-export tests retain their assertions. The command set in the first increment record passes again: 241 emulator tests plus 13 shared tests, strict workspace Clippy, locked build, all-feature test compilation and formatting. Both required reviewers report no actionable findings.

*Boundary remaining at this increment:* raw egui input and panel geometry still travelled to the worker's native adapter, so this extraction alone did not meet the complete input handoff boundary. The third increment below addresses that gap. Tests executed headlessly but still compiled existing GUI dependencies. Physical keyboard/window behavior and the German Option+L report were not revalidated or changed.

== Third increment record - AR-2 translated worker input

*Baseline and scope:* revision `96c1a32f1abe5ad7b07b190269be562a40a92b09`, with a clean starting worktree. Native interpretation moves to the GUI without changing supported mappings, machine behavior, supplied assets or dependencies.

*Boundary gained:* the GUI resolves native key/text pairing, widget ownership and panel hit tests before submitting an `InputFrame`. The frame contains only ordered neutral `InputAction` values, modifier lines, focus and a reset-prior-input flag. The worker owns `KeyboardState` and `InputSequence`, with no native keyboard adapter, front-panel view, egui key/event/modifier or rectangle types in its input path.

`ManualKeyId` identifies a logical host key without exposing its native type or confusing it with a Tek code. Only an accepted press changes worker ownership. This matters when modal mode discards a press: a later release of that discarded key must not release another accepted key, and the original key must still be releasable. The GUI therefore translates identities but does not predict worker ownership.

*Preserved ordering:* event-time modifiers precede their associated key operations; frame modifiers reconcile afterward. Reset-prior-input releases/cancels existing input while still allowing newly interpreted panel pointers. Empty frames coalesce only when their effective focus/modifier/reset state is equivalent; real release actions remain ordered. Focus loss still clears input before subsequent GUI-driven catch-up, including the modal path. Sequencing retains its existing 100 ms phases.

*Evidence:* the complete regressions passed with 242 emulator and 13 shared tests, along with strict workspace Clippy, locked build, all-feature consumer compilation and formatting. A native GUI/production-inline test proves ignored modal presses do not steal release identity. The test review requested stronger reset-transition retention and modal focus-cleanup evidence. The queue test now proves the first reset precedes the retained release; a new inline test proves key and modifier lines clear while modal suspension is still active. After those test-only additions, all 94 native binary tests and strict workspace Clippy passed again, bringing validated emulator cases to 243. Both required reviewers have no remaining actionable findings.

The focused follow-up command was:

```sh
cargo test --locked -p tek4051-emulator \
  --bin tek4051-emulator --quiet
```

AR-2's neutral input and adapter exit evidence is recorded. At this increment, `Snapshot.image` and cached presentation images still used egui types. The fourth increment below addresses that session boundary. Host I/O/codecs remain AR-4 work. No physical native window or keyboard check was performed; the German Option+L report remains unchanged.

== Fourth increment record - AR-3 neutral session snapshots

*Baseline and scope:* revision `e0a0096c7190f32f545b168a3b4a979d08571c99`, clean starting worktree. Separate session data from native presentation without changing machine/library code, supplied assets, input mappings or dependencies.

*Boundary gained:* `emulation.rs` retains concrete commands, `EmulationState` and `Snapshot`, with no egui types. Snapshots contain immutable grayscale `Arc<[u8]>` pixels and their machine display revision. A cache shares unchanged pixels across fresh metadata publications and copies only a changed revision; screen-copy requests retain their independent event-time bytes and IDs.

`native_backend.rs` owns the existing bounded desktop worker transport and a concrete presentation cache. `PresentedSnapshot` pairs a neutral snapshot with its matching green egui image in the same publication, avoiding an independent image/state race. The initial image cache moves with the session into the worker. Changed-frame conversion still runs before acquiring the short publication lock and never during repaint. The GUI uploads the prepared image allocation directly. No generic publisher, plugin interface or forwarding facade was added.

*Cost and ownership:* the neutral view now retains an additional grayscale allocation for each live revision, alongside its native color image. Unchanged revisions reuse both allocations. That per-revision representation cost removes the session's GUI dependency and lets session/capture behavior be tested without a native adapter. Session methods exposed to the sibling worker remain crate-local; no machine API was widened. Inline and join helpers reside in a separate test-only file.

*Evidence:* all 95 native binary tests pass. A direct session test checks unchanged allocation reuse, new erase pixels/revision, old snapshot/capture immutability and cancellation. A native preparation test checks complete dimensions, green pixel values, metadata-only image reuse and changed/repeated revision behavior. Existing texture-upload, x100 interaction, real gated export, queue/focus, acknowledgement, startup/contention and shutdown regressions pass without relaxing their bounds. Strict workspace all-target/all-feature Clippy, locked build, all-feature consumer compilation and formatting pass. Both required reviewers report no findings. The implementation review notes the preserved AR-4 risk: slow worker-side host I/O can delay command draining, although GUI polling remains nonblocking.

```sh
cargo test --locked -p tek4051-emulator \
  --bin tek4051-emulator --quiet
cargo clippy --locked --workspace \
  --all-targets --all-features -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

*Boundary remaining at this increment:* the session still lived in the native binary and called binary-root host-file helpers. The fifth increment removes those file calls. The desktop backend remains presentation-specific, independent non-GUI consumption is not established, and existing GUI dependencies still compile. This was bounded AR-3 evidence, not integrated acceptance.

== Fifth increment record - Native file effects

*Baseline and scope:* source revision `0473bdff15a384988539187910b83edc7cba5ad8`, preserving four existing design/handoff/history edits. External Git activity incorporated those documents during implementation. This is AR-4 preparation supporting independent AR-3 consumption, not acceptance of a later grade out of order.

*Boundary gained:* `EmulationState.apply` now returns an optional concrete `FileEffect`; it opens no host files and invokes no binary-root file helpers. Numbered export effects own their selected file snapshot; screen-save effects own the retained capture pixels. Archive effects carry paths and use the same suspended machine. `host_io.rs` owns filesystem execution and the unchanged PNG/file/archive writing helpers.

The native worker and inline adapter share one `apply_command`: prepare an effect, execute it on that owner, and return a `FileOutcome` to the session before another command or time advancement. The session retains notice/capture-overflow policy. No extra queue, asynchronous job, generic effect framework or persistence policy was added. Keeping archive execution on the owner preserves streaming and acknowledged store consistency without copying the whole store.

*Preservation:* an unknown capture is still rejected before consuming a valid request. A valid save still removes its selected pending request before writing, and a write failure remains a visible, nonfatal notice. Export preparation failures keep their existing notices. Successful/failed archive imports use the existing transactional machine API. Modal publication, file-time erase accounting, input ordering, bounded transport and shutdown handling are unchanged; slow I/O can still delay command draining, not GUI polling.

*Evidence:* the initial 97 native binary tests passed with strict workspace all-target/all-feature Clippy, locked build, all-feature consumer compilation and formatting. The direct-session test proves no file or completion notice appears until the returned effect is executed, rejects an unknown capture without losing the real one, and decodes the earlier captured pixels after a live erase. The inline/native test proves successful archive replacement and explicit read/write failures with storage retained.

The implementation review reported no actionable findings. The test review requested actual-worker ordering/running-state and archive-export completion coverage. A new real-worker test exports and round-trips an archive before a queued acknowledgement, checks restored modal/running state, reports failed archive creation while observing continued CPU progress, and verifies exact retained file bytes afterward. All 98 native tests, formatter and strict workspace Clippy then passed. The follow-up closes both findings and confirms the existing gated GUI export test supplies complementary blocked-I/O ordering coverage. Both required reviews have no unresolved actionable findings. Existing PNG/format, queue, focus, acknowledgement and lifecycle regressions retain their assertions and bounds.

The same binary/workspace commands listed in the fourth record were used. Unchanged library/shared suites were not rerun for this binary-only increment.

*Boundary remaining at this increment:* the session was independent of native file execution but still exposed mutable fields to its consumers. The sixth increment narrows that access. Reusable library consumption, host path/chooser/import preparation, clock sampling and codec ownership remain open; no GUI-free dependency build or migration acceptance was claimed.

== Sixth increment record - Private session consumer boundary

*Baseline and scope:* HEAD `45ceb42ac63cded16a65bf229c24662a2c92d098` plus the already-staged fifth increment and existing documentation edits. Preserve that work; this increment changes the unstaged session/consumer boundary without staging or committing.

*Boundary gained:* the existing `EmulationState` owns a private `SessionData` field containing its original data. Business methods remain on the session and use that storage directly; there is no second session object, forwarding facade, extra queue or heap allocation introduced by the container. Normal consumers receive no mutable machine/runtime/input field or reference.

The native worker uses explicit accepted-modifier advancement, temporary file-I/O suspension/completion, fatal failure and shutdown operations. File completion accounts for erase wall time while still suspended and restores the previous modal boundary. Failure/shutdown cleanup pauses execution, clears readiness and releases input; failure retains its error. `host_io.rs` uses session archive import/write streams rather than a machine reference, retaining transactional import and direct streaming output.

*Test boundary:* a separate `#[cfg(test)]` adapter supplies direct-field `Deref`/`DerefMut` only to existing private fixtures. This preserves their setup and assertions without mutable production accessors. It also means test-mode field resolution can conceal invalid normal-build access: production Clippy caught two missed multiline field rewrites after the test suite passed. Both were fixed. Validate normal production compilation as well as tests whenever changing this boundary.

*Evidence:* all 100 native tests pass. Two new tests exercise both pre-existing modal states over file time, frozen CPU plus completed erase and correct resumption, and fatal cleanup of a real held Shift/Control key with readiness/error/CPU assertions. Existing actual-worker archive success/failure, queued acknowledgement, gated GUI export/x100, capture, queue and lifecycle tests retain their assertions/bounds. Strict workspace all-target/all-feature Clippy, locked production build, all-feature consumer compilation and formatting pass. Both required Rust reviewers report no actionable findings; they reviewed source/tests without rerunning the reported validation.

The same validation commands listed in the fourth record were used, with normal-build Clippy/build explicitly preceding the final native test run. Unchanged library/shared suites were not rerun for this binary-only increment.

*Boundary remaining at this increment:* the narrowed session and its contracts still lived in the native binary. The seventh increment moves the shared contract types into the library. Reusable session execution and deliberate private-fixture placement remain open, alongside GUI-free compilation, host clock/codec/path decisions and physical acceptance.

== Seventh increment record - Library-owned emulation contracts

*Baseline and scope:* clean source `5fb0f43de8a1328881433235d29f691a60e2b46f`. This bounded step establishes shared type ownership before relocating private session execution and its native fixtures.

*Boundary gained:* `tek4051_emulator::emulation` now owns the existing `Command`, `Snapshot`, `FileEffect` and `FileOutcome` definitions in `emulation/mod.rs`. Native session, GUI, worker and host I/O consume those types directly from the library, not through the private binary session module. Variants, field types, owned data, pixel sharing and snapshot cloning are preserved; visibility is now public for these contracts only.

The library module contains no executor, host filesystem calls, widget types or mutable session/hardware reference. The native session still privately owns the machine/runtime/input data. There is no compatibility re-export, duplicated definition, generic transport, new crate or dependency. Explicit module paths distinguish the library contract area from the still-local native `emulation.rs`.

*Preservation and evidence:* existing command matching, effects/completion, capture identity, accepted input, native presentation, archive streaming and worker pacing algorithms are unchanged. The two changed test files only adjust imports; their bodies/assertions and all 100 native test cases remain intact. No new constant/layout tests were added for this source move. All 100 native tests, strict locked workspace all-target/all-feature Clippy, locked native build, all-feature consumer compilation and formatting pass. Initial module-resolution ambiguity and retained neutral-input imports were corrected before final validation. Both required reviewers report no findings. The test reviewer additionally reports 100 native and three library unit tests passing; the implementation reviewer did not rerun validation.

The same validation commands listed in the fourth record were used. Hardware integration/shared behavioral suites were not rerun; all library and interactive targets compile.

*Boundary remaining at this increment:* contracts were independently importable, but execution and private fixtures remained binary-local. The eighth increment moves the actual session and places native fixtures in its private test scope. GUI-free dependencies and host clock/codec/path decisions remain separate work.

== Eighth increment record - Reusable library session

*Baseline and scope:* clean source `5b149bbea0dc0ccf52b417fe43d108696fac0ef8`. External Git incorporated source changes during implementation; review must compare with this initial revision, not merely the later unstaged diff. No assistant staging or commit is part of this increment.

*Boundary gained:* the original implementation moves to `emulation/session.rs`, exported as `tek4051_emulator::emulation::EmulationState`. Native transport and host I/O use that one library owner. Its existing command, snapshot, time/lifecycle and archive stream methods are public, but machine/runtime/input data remains private. The logical realtime tick is exported for the actual native transport consumer. No worker, window, host file executor, state forwarding facade or second session implementation is added.

*Private fixtures:* the native adapter source moves from the executable root to `native.rs`; `main.rs` simply invokes its preserved startup. Under `#[cfg(test)]`, the library session loads that same native source as a descendant module. Existing fixtures therefore retain Rust-private access without a public testing hook or wider production fields. The old session field `Deref` is removed. The private widget fixture adapter alone projects data, and core/native fixture references use explicit data access.

The test-only self-crate alias preserves the native sources' library imports. Relative native module/test paths are explicit where needed. Native startup stays compiled in the headless harness but is not invoked: one narrow expected-dead-code annotation documents that limitation rather than suppressing the backend's unused methods or launching a window.

*Evidence:* all 100 previous native cases run exactly once inside the library suite, together with its three existing unit cases (103 passed). The initial two external public-API cases verify CPU/erase/grayscale publication, retained earlier capture after live erase, stale-ID rejection, nonfatal host completion/Pause and archive replacement/streaming.

The implementation review requested explicit machine-initialization and host clock/transition documentation. The constructor now states that callers reset for ROM boot or configure a custom program; it does not reset the supplied registers. Public timing documentation requires nondecreasing timestamps and accounting for the old state before Run/Pause changes. A third external case proves prepared registers survive construction, five seconds of pause do not execute, an equal resume timestamp advances nothing, and the next tick runs.

The test review requested exact retained payload evidence after failed archive validation and relocating test helpers out of the native implementation. The archive case now obtains a public export effect and checks the old bytes before successful replacement. Dialog/export/speed helpers move unchanged to `main/test_support.rs`, loaded only for private fixtures. All 103 library and three public cases, strict lint/build/all-feature compilation and formatting pass after these fixes. Both review follow-ups close their findings with no remaining actionable issues; Cargo was not rerun by the reviewers. Existing assertions, worker/FIFO/x100 bounds and startup logic are retained.

```sh
cargo fmt --all
cargo test --locked -p tek4051-emulator --lib --test emulation --quiet
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

All listed checks pass. The new public tests exercise the normal library API without widget calls or private state. Native regressions are now in `--lib`; testing only the thin binary would miss them. Hardware integration/shared behavioral suites were not rerun; all interactive consumers compile.

*Remaining work:* reusable headless session execution is now demonstrated, not a GUI-free dependency build or physical native certification. Next separate native-only Cargo dependencies while preserving default application behavior, then resolve remaining host clock/codec/path responsibilities. Migration grades, hardware/assets/mappings and the German Option+L issue are unchanged.

== Ninth increment record - Optional native dependency boundary

*Baseline and scope:* clean source `a428fce588de93c83a0063ffcd08c2b8fd0d367b`. Only the emulator package manifest, library overview/self-crate test alias and native fixture gate change. No execution algorithms, public contracts, fixture assertions, assets, dependency versions or lockfile change.

*Boundary gained:* `native` is enabled by default and enables optional `eframe`, `rfd` and PNG `image` dependencies. The executable requires `native`. Existing default application and regression behavior is preserved, while disabling defaults selects the same reusable library/session without native window, dialog, rendering or image packages. The native descendant fixtures and test alias require both test configuration and `native`; three core units and all three public session cases remain available without it. No separate crate, duplicated session or speculative abstraction is introduced.

*Preservation and evidence:* default validation passes all 103 library cases and three public cases using the eighth record's commands, including strict workspace lint, native build and all-feature consumer compilation. An offline normal/build/dev dependency graph explicitly rejects egui/eframe, window/rendering, dialog and PNG package names. A fresh workspace-local target directory independently builds, tests and lints the no-default configuration; its three core units and three public session cases pass. This is a fresh target build using existing cached registry sources, not a claim that dependencies were newly downloaded.

```sh
cargo tree --locked --offline -p tek4051-emulator \
  --no-default-features -e normal,build,dev \
  --prefix none --format '{p}'
```

With `CARGO_TARGET_DIR` set to the fresh directory:

```sh
cargo build --locked --offline -p tek4051-emulator --no-default-features --lib
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --lib --test emulation --quiet
cargo clippy --locked --offline -p tek4051-emulator \
  --no-default-features --all-targets -- -D warnings
```

*Scope of the guarantee:* core `jiff` time and `zip` archive dependencies remain. Headless downstream dependencies must set `default-features = false`. Cargo features are additive: another selected consumer can reenable `native`. The shared Flash Drive test package retains its native dependencies/default-enabled emulator dependency, so arbitrary whole-workspace tests are not GUI-free. The 100 native fixtures are deliberately absent without `native`, not replaced by the six core/public cases; default validation remains required.

Both required focused Rust reviewers report no actionable findings. They inspected the feature wiring and directly relevant tests/consumer manifests without rerunning Cargo. The test review confirms the headless cases exercise observable execution, timing, capture/error and transactional archive behavior, rather than merely testing feature constants.

*Remaining work:* this establishes a compiler/dependency boundary for headless consumers without certifying all AR-3 through AR-5 gates. Resolve remaining host clock/codec/path responsibilities and focused native acceptance separately. Existing worker/FIFO/x100 bounds are preserved, not physical-window certification. The emulator remains stopped; migration grades, hardware/assets/mappings and German Option+L are unchanged.

== Tenth increment record - Shared lossless archive JSON

*Baseline and scope:* clean source `bce84d84c20db8a53e4295b1760fe51f7ba14c71`. External Git incorporated source during the increment; reviews compare that immutable baseline with the worktree/index. No assistant staging/commits or unrelated reversal.

*Boundary gained:* the existing archive JSON parser and UTF-16 string encoder move out of the tape device into crate-private `flash_archive_json.rs`. The parser gains a constructor; only caller-used syntax operations become crate-visible. Source/offset and Unicode/whitespace helpers remain private. This is a shared interchange utility, not GUI/emulation policy: it has no machine, ZIP, filesystem or native dependencies. It introduces no public API, generic serialization framework, new crate or dependency.

Tape still owns file-number/type/usage/description validation, machine LIST/header protocol, ZIP entry pairing/limits, streamed payloads and atomic storage replacement. Those algorithms, error messages and native timing/ordering remain unchanged. This bounded step separates JSON string mechanics without claiming that all archive or filename responsibility has moved out of hardware.

*Evidence:* two new private tests live separately in `flash_archive_json/tests.rs`, available with or without `native`. Exact authored output checks quotes, backslashes, NUL, a valid Unicode pair and both isolated surrogate kinds. Independently authored escaped JSON verifies raw UTF-16 preservation; decoding the scalar output covers raw Unicode too. Invalid/incomplete escapes, raw controls, unterminated strings and trailing input retain explicit errors.

The existing 64 public GPIB cases pass in both configurations, including exact archive header/content bytes, malformed-input atomicity, expansion boundaries, transfer cancellation and machine LIST/filename behavior. Default validation passes 105 library cases plus three public session cases; no-default validation passes five core/codec units plus three public session cases. All original 100 native regressions retain their assertions and worker/FIFO/x100 bounds. Strict lint passes in both configurations, the native build and all-feature workspace consumers compile, and formatting passes. The final test-only raw-scalar assertion passes the focused no-default codec selector.

The implementation review reports no actionable findings without rerunning Cargo. The test review requested shorthand control/slash escape coverage. The existing preservation case now decodes independently authored `\/`, `\b`, `\f`, `\n`, `\r` and `\t`, checks exact UTF-16 units and asserts the encoder's exact shorthand output/unescaped slash. Both feature modes' focused codec tests and strict lint pass after that test-only refinement. The test reviewer separately reports two codec and 14 archive integration cases passing; the follow-up closes the finding without another test run. Both required reviews are clear.

```sh
cargo test --locked -p tek4051-emulator --lib --test gpib --test emulation --quiet
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --lib --test gpib --test emulation --quiet
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked --offline -p tek4051-emulator \
  --no-default-features --all-targets -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

*Remaining work:* host-clock sampling, ZIP/archive schema and filename placement, host adapters and physical native acceptance remain open. No CPU/display/machine or shared behavioral suite beyond the named selectors was executed; their consumers compile. This is AR-4 preparation, not a later-grade or migration acceptance. Assets, mappings, dependency versions and lockfile are unchanged; the emulator remains stopped.

== Eleventh increment record - Shared metadata and archive header schema

*Baseline and scope:* clean source `cf3595a8cf36a68792aa771833ae1c15c5eb8638`. Archive header schema validation/encoding and existing file-type/usage code/label definitions move from tape into shared crate-private `flash_drive.rs`. GPIB/tape imports use those single definitions; the library declares the new module. Existing public API/test fixtures, JSON algorithms, device operations and dependencies are unchanged.

*Boundary gained:* archive header decoding no longer belongs to the tape state implementation. The codec validates the existing numbered four-field JSON format and preserves raw UTF-16 descriptions. Encoding accepts existing optional type/usage and borrowed description metadata, not a private tape object or a whole-file copy. Its missing-type failure remains explicit. The enums are shared metadata, not a new application dependency from hardware; labels and byte-code rules are not duplicated.

Machine LIST parsing still owns its positional/byte interpretation and NEW/LAST usage policy. Archive schema retains its distinct rules: optional usage is not inferred from type, and nonempty usage on a NEW marker remains accepted. ZIP entry pairing/limits, archive payload streaming and atomic tape replacement remain untouched. This does not certify the full AR-4 host-I/O boundary or introduce a generic serialization framework.

*Evidence:* two new private tests in `flash_drive/tests.rs` run without native adapters. They decode independently authored 1/254-number headers, exactly 16 UTF-16 units with an isolated surrogate, NEW/TEXT and ASCII/empty usage; exact re-encoding preserves those distinctions. Invalid number bounds/leading-zero syntax, unsupported codes, 17-unit descriptions, invalid UTF-8 and missing export type return the original explicit diagnostics.

Default validation passes 107 library cases plus three public session and all 64 GPIB cases. No-default offline validation passes seven core/codec units plus the same three/64 cases. Existing native worker/FIFO/x100 and public LIST/archive/filename regressions retain assertions/bounds. Strict workspace all-feature lint and no-default all-target lint, native build, all-feature consumer compilation and formatting pass.

The implementation review reports no findings without rerunning validation. The test review requested field-cardinality coverage. The existing invalid-header case now rejects empty/multi-unit type fields and multi-unit usage with the original explicit diagnostics. Both feature modes' focused schema tests, strict lint and formatting pass after that test-only addition. The follow-up closes the finding without independently rerunning Cargo; both required reviews are clear.

```sh
cargo test --locked -p tek4051-emulator --lib --test gpib --test emulation --quiet
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --lib --test gpib --test emulation --quiet
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked --offline -p tek4051-emulator \
  --no-default-features --all-targets -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

*Remaining work:* host-clock sampling, ZIP stream/pair/limit and filename placement, host adapters and physical native acceptance remain open. Other CPU/display/machine/shared behavioral suites were not executed; their consumers compile. No assets, keyboard mapping, dependency versions, lockfile or migration grade changed; the emulator remains stopped.

== Twelfth increment record - Shared whole-storage ZIP codec

*Baseline and scope:* clean source `28df5b5b858112f264f1043d13eefd6059fff689`. Existing whole-storage ZIP writer/reader/entry-budget logic and limits move from tape/GPIB into shared crate-private `flash_archive.rs`. Public limit names under `machine::gpib` reexport the single codec-owned definitions, preserving existing callers rather than duplicating constants or changing the API.

*Boundary gained:* tape no longer imports ZIP types or interprets archive entry pairs. Export supplies a concrete borrowed file view through its sorted map iterator; the codec emits the original ordered JSON-header/content names and streams payloads without a whole-store clone. The exact-size iterator preserves explicit empty-store rejection without buffering all files. The codec consumes caller order; canonical sorting remains with the tape store.

Import validates every pair under the original compressed/entry/total budgets, ignores entry names and keeps the last duplicate number. It returns owned header/payload records. Tape moves those fields into private files, then applies its original selection, snapshot, cursor and pending-write reset only after success; GPIB transfer cancellation remains unchanged. Converting the intermediate map may allocate metadata nodes, but adds no payload/description clone. The earlier selected current-file clone is preserved, not newly introduced. No traits/frameworks, dependency versions, archive policy or device algorithms are added.

*Evidence:* two private `flash_archive/tests.rs` cases use no machine/native adapter. They verify exact ZIP names and header/content bytes, including raw binary bytes, isolated UTF-16/quote/NUL descriptions and empty LAST content; decoded metadata matches those independently specified values. Empty export, an unwritable fixed sink and a malformed later header remain explicit failures. The injected sink emits the existing ZIP library's expected drop-time WriteZero diagnostic; no suppression or workaround was introduced.

Default validation passes 109 library cases, three public session cases and all 64 GPIB cases. No-default offline validation passes nine core/codec units plus the same three/64 cases. Existing public archive tests verify exact compressed/expanded/count limits, arbitrary names/out-of-order duplicate handling, retained state on failure and OLD/READ/WRITE cancellation. Existing native worker/FIFO/x100 bounds remain unchanged. Strict lint in both modes, native build, all-feature consumer compilation and formatting pass.

Both required focused Rust reviewers report no actionable findings. They confirm preserved ordering/pairing/limits/diagnostics, moved rather than cloned payloads, atomic tape commit and proportionate separate-file tests. Neither independently reran Cargo; the implementation reviewer additionally reports a clean scoped whitespace check. The intentional failed-sink diagnostic is not a review finding.

```sh
cargo test --locked -p tek4051-emulator --lib --test gpib --test emulation --quiet
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --lib --test gpib --test emulation --quiet
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked --offline -p tek4051-emulator \
  --no-default-features --all-targets -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

*Remaining work:* host-clock sampling, host filename rules, broader host-adapter organization and physical native acceptance remain open. Existing machine/session stream APIs still delegate to the codec; this is not a new headless public storage API or AR-4/AR-5 acceptance. Other hardware/shared behavioral suites were not executed; consumers compile. Assets, mappings, dependencies, lockfile and migration grades are unchanged; the emulator remains stopped.

== Thirteenth increment record - Shared filenames and fixed-width metadata

*Baseline and scope:* clean source `b3bef37c2c41df24d5327480d6037783454ccf04`. The existing fixed-width UTF-16 formatter, header/integer parsers, host-name checks and export error definition move from tape to `flash_drive.rs`. The public GPIB export-error path reexports the same variants/diagnostics. No public API, additional format policy or filesystem operations are introduced.

*Boundary gained:* LIST and host export cannot drift into duplicate formatting implementations. The shared formatter retains type/usage widths, UTF-16 description padding and actual byte sizes. Host names alone enforce valid Unicode and reject slash/backslash/NUL. Canonical file lookup and requested-number validation stay in tape; requested and stored numbers remain distinct when passing metadata to the codec. The private file adapter only unpacks existing fields, without copying payloads.

The shared parser preserves first-letter/positional metadata rules, bracket removal, 16-unit truncation/padding, ignored size text and reference integer whitespace/sign/hex/trailing-input behavior. Out-of-range numeric headers still decode with no replacement number, and NEW/LAST usage is not supplied by that parser. Tape applies its original retained-number/usage update, LIST low-byte/sentinel behavior, batch import/selection and FIND range policy. This is a pure reusable protocol/interchange utility, not an application orchestration dependency from hardware.

*Evidence:* two added cases in the existing private `flash_drive/tests.rs` verify exact Unicode filename padding/shared LIST units, decoded metadata and legacy NEW/out-of-range/ignored-size/bracket behavior. Slash, backslash, NUL and lone-surrogate descriptions retain raw LIST units but return explicit invalid-filename errors; 17-unit host descriptions remain rejected. The earlier two schema cases and all existing public assertions are unchanged.

Default validation passes 111 library cases, three public session cases and all 64 GPIB cases; no-default offline passes eleven core/codec units plus the same three/64 cases. Strict lint in both configurations, native build, all-feature consumer compilation and formatting pass. Existing LIST/import/export/FIND state and native worker/FIFO/x100 regressions retain their assertions/bounds. An initial misplaced module block prevented formatting; it was corrected before this successful validation. The inherited intentional ZIP failed-sink diagnostic remains expected.

Both required focused Rust reviewers report no findings. They confirm the cohesive shared codec, preserved requested/stored-number handling and public error paths, grounded comments and meaningful separate-file tests. Neither independently ran Cargo; the implementation reviewer additionally reports a scoped whitespace check passing.

```sh
cargo test --locked -p tek4051-emulator --lib --test gpib --test emulation --quiet
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --lib --test gpib --test emulation --quiet
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked --offline -p tek4051-emulator \
  --no-default-features --all-targets -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

*Remaining work:* host-clock sampling, broader host-adapter organization and physical native acceptance remain open. Other hardware/shared behavioral suites were not executed; consumers compile. No assets, key mapping, dependencies/lockfile or migration grade changed; the emulator stays stopped. AR-4/AR-5 acceptance is not claimed.

== Fourteenth increment record - Injectable RTC host sampling

*Baseline and scope:* source baseline `0bac223`, after user-reported `git clean -dxf`. The original system timezone/time lookup moves from GPIB into `host_clock.rs`. Hardware retains the RTC calendar encoder and on-demand dispatch; `Machine::set_rtc_clock` configures a concrete function pointer returning a Jiff calendar value or error.

*Boundary gained:* GPIB no longer performs system timezone lookup or reads `SystemTime`. Public `host_clock::local_datetime` owns the host sampling operation, and native startup explicitly installs it before reset. The default controller also binds the same sampler for compatibility with existing `Machine::default()` callers. Other hosts can supply deterministic values before constructing an emulation session. Sampling still occurs on address-2 command `0x6d`, not when installing the callback or receiving buffered bytes. Setter calls do not clear input or handshake state, and CPU reset retains the sampler. No closure storage, generic clock traits, captured state or dependency changes are introduced. The compatibility default remains a host binding within machine construction.

*Evidence:* two new public cases in `tests/gpib.rs` verify exact single-digit-year/millisecond bytes after reset, last-byte EOI, buffered-response stability when replacing the callback, ignored unrelated commands and propagation of the supplied clock diagnostic through the existing bus error. Existing default host-local interval, invalid-system-timezone and private calendar-format regressions remain unchanged.

Default validation passes 111 library cases, three public session cases and 66 GPIB cases; no-default offline passes eleven library cases plus the same three/66 cases. Strict workspace all-target/all-feature and headless all-target lint, native build, all-feature workspace consumer compilation and formatting pass. These checks were run after the cleanup; other hardware/shared behavioral suites were compiled, not executed. The inherited intentional ZIP failed-sink diagnostic remains expected.

Both required focused Rust reviewers report no findings and did not independently run Cargo. The implementation reviewer confirms preserved sampling/formatting/error/handshake behavior and native-worker compatibility. The test reviewer confirms meaningful reset, byte/EOI, buffered-response and injected-error coverage. Function pointers deliberately cannot capture per-machine state; a future stateful host would need a separate design decision.

```sh
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --test gpib rtc_ --quiet
cargo test --locked -p tek4051-emulator --lib --test gpib --test emulation --quiet
cargo test --locked --offline -p tek4051-emulator \
  --no-default-features --lib --test gpib --test emulation --quiet
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked --offline -p tek4051-emulator \
  --no-default-features --all-targets -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked --workspace --all-features --no-run
cargo fmt --all -- --check
```

*Remaining work:* broader host-adapter organization and physical native acceptance. The default machine remains host-clock-backed; native startup explicitly installs the same sampler, and deterministic callers replace it. AR-4/AR-5 acceptance and migration advancement are not claimed. Assets, keyboard mappings and dependencies/lockfile are unchanged; emulator remains stopped.

== Fifteenth increment record - Host file import preparation

*Baseline and scope:* source baseline `036d1841c250aa910b10a8bbe5772981bb61a93b`. Move loose-file and bounded ZIP selection preparation from `native.rs` to private native adapter `host_import.rs`. Native dialogs stay in the presentation module; file-effect execution remains in `host_io.rs`.

*Boundary gained:* `host_import.rs` opens selected paths, reads loose files, iterates ZIP entries and builds owned `FlashDriveFile` records before commands reach the machine. The existing per-selection entry and expanded-byte budgets, compressed archive-size check, basename extraction, partial-success handling and operation-specific error text move unchanged. The importer accesses no session, machine, dialogs or GUI. The existing background file job and worker-command ordering remain unchanged; this is module ownership, not a threading or I/O scheduling change.

*Evidence:* existing private native tests were kept in their separate test file and redirected to the new module. They continue to cover non-Unicode/unreadable paths, invalid/empty archives, compressed-size rejection without consuming batch budget, per-entry and aggregate expansion limits, path basenames, partial errors and selection-wide limits. A new focused case sends loose, ZIP and missing loose paths through one normal selection batch, asserting mixed-path ordering, exact bytes and a separate explicit read failure.

Validation passes: `cargo fmt --all`; default tests with 112 library, three public session and 66 GPIB cases; offline no-default tests with eleven library and the same three/66 cases; strict workspace all-target/all-feature and package no-default Clippy; locked native build; all-feature workspace test compilation; and formatting check. The inherited ZIP failed-sink tests emit the expected WriteZero diagnostic. Other behavioral suites were compiled, not executed.

Both focused Rust reviewers report no remaining actionable findings. Test review's missing mixed loose-file/ZIP dispatch coverage is closed by the new selection test. No production behavior, public API, dependency, asset, mapping or migration-grade change. Slow host effect writes may still delay worker commands; broader adapter organization and physical native acceptance remain open.

== Sixteenth increment record - Native dialog adapter isolation

*Baseline and scope:* source baseline `3035f1d79fd3c2e1bdb8bf53c13cc7e346d45783`, with host import preparation already isolated in `host_import.rs`. Move rfd file/save/confirmation dialog construction out of both `native.rs` and egui presentation `main/gui.rs` into private native adapter `host_dialogs.rs`.

*Boundary gained:* the dialog adapter owns the exact titles, default names, filters, confirmation prompt and accepted/cancelled return values. UI code still decides when to schedule the modal file job; the dialogs remain on that background job. Returned paths flow to `host_import.rs` for reads or `host_io.rs` for writes. The extraction changes neither modality/worker ordering nor filesystem access, and does not claim that physical chooser behavior has been certified.

*Evidence:* existing private UI tests exercise injected import/export/confirmation choices and the same file-job transitions; no test requires an actual desktop chooser. Default validation passes 112 library cases, three public session cases and all 66 GPIB cases. No-default offline passes eleven library, three public session and 66 GPIB cases. Strict workspace all-target/all-feature and package no-default Clippy, locked native build, all-feature workspace test compilation and formatting pass. The inherited failed-sink ZIP tests emit their expected WriteZero diagnostics. Both architecture documents compile; temporary PDFs are removed.

Focused Rust code review reports no findings. No new user-visible behavior, public API, dependency, asset, mapping or migration-grade change. Actual OS dialog interaction, broader host-adapter organization and physical native acceptance remain open; effect writes can still delay worker command draining.

== Seventeenth increment record - Emulation input and runtime grouping

*Baseline and scope:* source baseline `91792279569640645cfdbb0401d79d2618f019ca`; the worktree already contained chat handoff/history updates, with no Rust or design-document edits. Move `emulation_input.rs` and `host_runtime.rs` under `emulation/`, retain their former public root paths, and update the session and input implementation to use the grouped sibling modules. No behavior, dependency, thread, worker, or machine changes are in scope.

*Boundary gained:* `emulation/input.rs` now owns framework-neutral input sequencing beside `emulation/runtime.rs`, which schedules deterministic machine work from supplied elapsed time; `emulation/session.rs` uses these nested modules directly. `lib.rs` publicly re-exports them as `emulation_input` and `host_runtime`, preserving existing callers. No state was copied and no runtime wrapper or new abstraction was introduced. Tests and external consumers continue to use the existing paths.

*Evidence:* the complete default emulator package suite passes 263 tests. Offline no-default-feature validation passes the 11 library, 3 host-runtime, 2 input-sequencing and 19 emulation cases; strict all-target/all-feature Clippy and formatting pass; the shared Flash Drive consumer test targets compile. The inherited failed-sink ZIP tests emit their expected `WriteZero` diagnostic. No test source changed.

```sh
cargo test --locked -p tek4051-emulator --quiet
cargo test --locked --offline -p tek4051-emulator --no-default-features \
  --lib --test host_runtime --test emulation_input --test emulation --quiet
cargo clippy --locked -p tek4051-emulator --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --locked -p tek4051-flash-drive-tests --no-run
typst compile --package-cache-path target/typst-packages \
  dev/docs/tek4051/web/design/design.typ target/emulation-grouping-design-check.pdf
typst compile --package-cache-path target/typst-packages \
  dev/docs/tek4051/web/design/graded-architectural-refactoring-guide.typ \
  target/emulation-grouping-guide-check.pdf
git diff --check
```

Both architecture documents compile and `git diff --check` passes. The focused Rust Code Quality Reviewer reports no findings; no test reviewer was needed because no tests changed. This incremental move does not claim an AR-grade exit gate.

*Remaining work:* organize host/GUI adapters in separate reviewed increments. Preserve the RTC compatibility seam; keep `native_backend.rs` explicit as a bridge unless a concrete dependency benefit justifies changing it.

== Eighteenth increment record - Shared Flash Drive codec grouping

*Baseline and scope:* clean worktree at source baseline `b0201ffe0d99ce8ccecf9741d6ac65b8c50afa9f`. Move the shared metadata/filename/header codec, JSON string codec and whole-storage ZIP codec under `flash_drive/`, along with their private test modules. Keep GPIB tape state and transfer policy in `machine/gpib/tape.rs`; preserve the public GPIB limit and export-error paths. No codec algorithm, data format, resource limit, storage behavior, dependency or file I/O changes are in scope.

*Boundary gained:* `flash_drive/mod.rs`, `flash_drive/archive_json.rs` and `flash_drive/archive.rs` now form one shared domain group. Their private regression modules move beside them. Machine GPIB consumers import archive helpers from that group and continue re-exporting the existing public constants and `FlashDriveExportError`. The ZIP codec still borrows export payloads and returns validated owned import records; the tape remains responsible for committing storage and its transfer-state policy.

*Evidence:* the complete default emulator package suite passes 263 tests. Offline no-default-feature validation passes the library codec tests and 66 GPIB plus 25 machine integration tests. Strict all-target/all-feature Clippy, formatting and compilation of the shared Flash Drive consumer tests pass. Both Rust reviewers report no findings; the test reviewer confirms the relocated test assertions and coverage are unchanged. Expected failed-sink ZIP tests emit `WriteZero`.

```sh
cargo test --locked -p tek4051-emulator --quiet
cargo test --locked --offline -p tek4051-emulator --no-default-features \
  --lib --test gpib --test machine --quiet
cargo clippy --locked -p tek4051-emulator --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --locked -p tek4051-flash-drive-tests --no-run
typst compile --package-cache-path target/typst-packages \
  dev/docs/tek4051/web/design/design.typ target/flash-drive-group-design-check.pdf
typst compile --package-cache-path target/typst-packages \
  dev/docs/tek4051/web/design/graded-architectural-refactoring-guide.typ \
  target/flash-drive-group-guide-check.pdf
git diff --check
```

Both architecture documents compile and `git diff --check` passes. No behavior, dependency, asset, keyboard mapping or migration grade changed. Host/GUI organization and physical native acceptance remain open.

= Grade AR-0 - Preservation baseline

*Goal:* define what must remain unchanged and resolve the decisions needed for the first bounded increment.

*Work:*

- Record the starting revision and relevant uncommitted changes. Preserve unrelated local work; a clean worktree or an assistant-created commit is not a prerequisite.
- Map current production and test consumers, including the shared Flash Drive session/worker tests. Identify each responsibility's owner, permitted dependencies and externally used APIs.
- Run the existing relevant checks and record exact commands, results, limitations and known failures. Do not generate replacement expectations from the refactored implementation to make tests pass.
- Preserve CPU/memory/PIA behavior, ROM bytes and mappings, display intensities and erase behavior, keyboard ordering/modifiers, device transfers and storage state. Include pause/reset/error behavior, macro cancellation, file outcomes and modal acknowledgements.
- Record the existing x100 and blocked-I/O responsiveness tests, their executable latency bounds and environment/build conditions. Separate headless widget timing from physical window responsiveness.
- Record the agreed first source grouping and input boundary. List unresolved host-clock compatibility and crate decisions with the grade that needs each answer. Do not introduce provisional framework layers merely to avoid deciding ownership.

*Exit gate:* an agreed, reproducible Rust preservation baseline and bounded first increment, with no silent failures or unresolved ownership questions affecting that increment. Migration gaps remain separately listed and do not become refactoring requirements.

= Grade AR-1 - Cohesive machine area

*Goal:* collect machine responsibilities behind a deliberate module boundary without changing their behavior.

*Work:*

- Group CPU, bus/memory, PIAs, ROMs, tube/display, machine keyboard state and machine-level I/O/device behavior in the machine area.
- Keep hardware memory mapping, interrupt ordering, device protocol and display semantics there. Application lifecycle/pacing and host adapters do not belong there merely because they currently share the library.
- Use Rust visibility and existing APIs to express the boundary. Retain justified component-level access for CPU tests, debugging and current consumers; do not force every use through a large forwarding facade or expose mutable internals to the GUI.
- Update directly dependent imports and test declarations together. Temporary compatibility re-exports need an actual consumer and a removal or retention decision, not an indefinite duplicate public surface.
- Preserve public-API tests in the relevant package's `tests/` directory and private-access tests in separate private test-module files.

*Immediate value:* machine ownership and dependencies become discoverable in one area; machine changes no longer require navigating application presentation code.

*Exit gate:* the grouped machine builds and its existing behavioral tests and relevant consumers pass. It imports no GUI/application modules. Source grouping and visibility, not folder names alone, demonstrate the boundary. Any remaining host-clock or codec coupling is explicitly assigned to AR-4.

= Grade AR-2 - Separate input responsibilities

*Goal:* end host-framework interpretation before input reaches machine behavior.

*Work:*

- Keep OS/egui events, key/text pairing, layouts, shortcuts, widget ownership, focus and pointer hit testing in the application input adapter.
- Translate into the smallest framework-neutral operations needed by emulation, reusing existing Tek key identifiers and modifier concepts where suitable. Do not create a generic event bus.
- Keep Tek key state, modifier lines, PIA effects and keyboard interrupts in the machine. Keep timed taps, Auto Load and directory-launch scripting in application emulation.
- Separate front-panel widget/hit-region handling from its timed sequencing. A worker need not receive widget rectangles to deliver an already interpreted panel action.
- Retain meaningful tests for text/key pairing, native-owned repeats/releases, focus-loss cleanup, held modifiers, manual/macro cancellation, rapid taps and the existing 100 ms press/release boundaries. A move alone needs existing tests relocated, not new tests asserting the new file layout.

*Immediate value:* host-layout defects can be investigated independently of PIA behavior, and scripted/manual machine input can be exercised without synthesizing GUI events.

*Exit gate:* machine input and timed sequencing can be tested using neutral operations without egui event, key or rectangle types. Existing keyboard/PIA and real-ROM input outcomes are unchanged; GUI adapter tests still cover native ownership and translation.

= Grade AR-3 - GUI-independent emulation

*Goal:* make the emulation session independently usable while preserving responsive presentation.

*Work:*

- Put machine ownership, runtime coordination, lifecycle, scripted input and worker command/snapshot behavior in application emulation. Separate deterministic elapsed-time advancement from the worker's real-time pacing and overload coalescing.
- Remove egui-specific events, rectangles, textures and color-image types from session APIs and state. Preserve display revision/data semantics and event-time screen captures.
- Assign display conversion to an application presentation adapter and execute it off the repaint path. Removing GUI types from emulation must not reintroduce expensive per-pixel conversion during GUI updates.
- Retain bounded command delivery, ordered releases, safe idle-frame coalescing, nonblocking latest-state reads and explicit saturation/disconnection errors.
- Preserve lifecycle/readiness request acknowledgements, exclusive mutable ownership, wall-time erase completion and shutdown signaling. Never join the worker or wait for its machine lock inside GUI update/drop.
- Keep deterministic test adapters on the same production session logic; also exercise the real worker so inline tests cannot conceal transport or scheduling regressions.

*Immediate value:* the session supports headless tests and non-GUI consumers without depending on widget details, while presentation changes cannot alter machine execution rules.

*Exit gate:* emulation tests run without a GUI context and its API contains no GUI-framework types. Real-worker progress, pause/reset/shutdown, saturation, contention, x100 GUI interaction and stale-acknowledgement regressions pass. Their existing bounds are not relaxed merely to accommodate the refactor.

= Grade AR-4 - Application I/O and host effects

*Goal:* separate host effects from machine-level I/O without changing device or user-visible contracts.

*Work:*

- Keep GPIB addressing, byte/EOI ordering, tape records, selection, FIND/OLD snapshots, cursors and pending-write state in the machine. Those are device semantics, not GUI storage operations.
- Put file choosers, host paths, filesystem access, PNG writing and import/export orchestration in application adapters. Reuse the current supported formats and validation limits; add no new format or persistence policy.
- Keep shared metadata, JSON, filename and archive interchange codecs in `flash_drive/`. Distinguish machine-visible LIST/header protocol use from host archive/filename interchange; dialogs, host file handles and filesystem operations must not leak into machine behavior.
- Resolve host-clock sampling separately from RTC protocol/formatting. Supply host values through the smallest useful boundary while preserving current local-time formatting and explicit failures; do not add a generic clock framework without need.
- Preserve the acknowledged suspended-store boundary before choosers, stable export names/payloads, captured screen-copy identities and completion/error notices. Single-file and whole-store export must not observe different store versions across a modal operation.
- Keep slow work off the GUI thread. An active worker-side read/write may still delay applying queued Pause/Reset; the window must remain interactive and the result must eventually be explicit.

*Immediate value:* file and clock failures can be tested without widgets, host-effect ownership is clear, and device behavior does not depend on desktop interaction.

*Exit gate:* machine behavior performs no host file selection/open/create operations or path policy. Remaining pure codec/host-value interfaces have an agreed owner. Existing format round trips, state-retention, malformed-input, cancellation, modal/erase and export-snapshot tests pass. A real blocked-I/O test synchronizes on I/O entry before checking GUI interaction and verifies exact output and completion after release.

== Native host-adapter file grouping (15 November 2026)

*Baseline and scope:* starting from `7d7292f`, moved the three existing native host adapter files into `host/dialogs.rs`, `host/import.rs`, and `host/io.rs`. The implementations, tests, error paths, and formats are unchanged; `native.rs` remains their declaring parent.

*Boundary and complexity decision:* `native.rs` uses `#[path]` declarations so the adapters remain sibling modules with their existing `pub(super)` visibility and consumer paths. A nested `host` module would require widening visibility or adding wrappers for native UI, worker, and test consumers, which this file-organization increment does not justify. The physical move adds no runtime dispatch, allocation, copying, synchronization, dependencies, or crate boundary. `host_clock.rs` remains at its compatibility seam, and `native_backend.rs` remains the integration bridge.

*Validation:* `cargo test --locked -p tek4051-emulator --quiet` passed all 263 tests; `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, `cargo test --locked -p tek4051-flash-drive-tests --no-run`, and staged/unstaged `git diff --check` passed. Rust Code Quality and Rust Test Quality reviewers reported no findings. No native GUI smoke test was performed.

*Result and remaining work at this increment:* the file-system grouping for dialogs, selected-file reads, and file effects is complete without changing module scope. GUI directory grouping and physical native acceptance remained open; the RTC compatibility seam and `native_backend.rs` bridge were unchanged.

== Native GUI file grouping (15 November 2026)

*Baseline and scope:* starting at `6dfb8d4`, moved native presentation and its private adapters/tests from `main/` and the package root into `gui/`: `main/gui.rs` to `gui/app.rs`, `front_panel.rs` and its tests, `keyboard_input.rs` and its tests, `main/inline.rs`, and `main/gui_tests.rs`. The implementations and test assertions are unchanged.

*Visibility and complexity decision:* `native.rs` continues declaring `gui`, `front_panel`, and `keyboard_input` as sibling modules with explicit `#[path]` attributes. This preserves current `pub(super)` access and sibling relationships without a nested wrapper or broader visibility; `gui/app.rs` still owns the same inline/test child modules. The moves change source paths only and add no runtime dispatch, allocation, copying, synchronization, dependencies, or crate boundary.

*Validation:* `cargo test --locked -p tek4051-emulator --quiet` passed all 263 tests; `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, `cargo test --locked -p tek4051-flash-drive-tests --no-run`, both architecture Typst compilations, and staged/unstaged `git diff --check` passed. Rust Code Quality and Rust Test Quality reviewers reported no findings. No native GUI smoke test was performed.

*Result and remaining work:* GUI presentation code and its private regression adapters are physically grouped under `gui/`; machine/emulation ownership, public APIs and behavior remain unchanged. `native_backend.rs` stays as the worker/image-preparation bridge, and `host_clock.rs` stays at its compatibility seam. Physical native acceptance remains open.

== Host clock implementation file grouping (15 November 2026)

*Baseline and scope:* starting at `d8352c8`, moved the unchanged host-local calendar sampler from `host_clock.rs` to `host/clock.rs`. `lib.rs` continues to export the public `host_clock` module name with an explicit `#[path]` declaration. `machine/gpib.rs` and native startup keep their existing call paths.

*Compatibility and complexity decision:* the existing `Machine::default()` function-pointer binding and native startup sampler are unchanged, as are timezone/system-clock error propagation and RTC response formatting. This is a physical file move only: no crate dependency, module visibility, public item, allocation, dispatch, or synchronization change. The public module seam is preserved; removing the machine's host-bound default remains a separate design decision.

*Validation:* `cargo test --locked -p tek4051-emulator --quiet` passed all 263 tests; no-default library, session, and GPIB selectors passed 80 tests. `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, and `cargo test --locked -p tek4051-flash-drive-tests --no-run` passed. Rust Code Quality Reviewer found no issues. Both architecture Typst documents compile; native GUI smoke testing was not performed.

*Result and remaining work:* the clock implementation now resides with the host adapters while existing Rust consumers retain `crate::host_clock::local_datetime`. No tests or assertions needed modification. Physical native acceptance and the intentional `native_backend.rs` bridge remain.

== Flash Drive directory presentation grouping (15 November 2026)

*Baseline and scope:* starting at `c8653a7`, moved the directory renderer and its action result from `native.rs` into `gui/flash_directory.rs`. `gui/app.rs` declares the view as a child module and remains responsible for holding prompt/selection state and converting returned UI intent into session commands.

*Ownership and complexity decision:* the directory code now shares the GUI source area with the app, panel and keyboard adapters. It renders rows, handles its focus/navigation input, and reports activation intent; it does not send emulation commands or acquire new machine state. The change adds one file/module and no runtime dispatch, allocation, copy, synchronization, dependency, or public API. The existing GUI regression suite covers selection, launch, focus and prompt-readiness behavior.

*Validation:* `cargo test --locked -p tek4051-emulator --quiet` passed all 263 tests; `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, `cargo test --locked -p tek4051-flash-drive-tests --no-run`, and `git diff --check` passed. Rust Code Quality review found no substantive issue; its formatting finding was fixed and formatter checks passed. No native GUI smoke test was performed.

*Result and remaining work:* the directory view now resides under `gui/`, while the native composition module retains event-loop and startup responsibilities. Device/tape behavior and command sequencing are unchanged. Physical native acceptance remains open; `native_backend.rs` remains the explicit worker/image-preparation bridge.

== Shared CPU speed policy (15 November 2026)

*Baseline and scope:* starting at `cfc2b81`, consolidated the identical speed-multiplier arrays previously defined in `native.rs` and `emulation/session.rs` into public `emulation::SPEED_FACTORS`. The session uses the shared list to validate `Command::Speed` indices; the GUI uses it to display and cycle the corresponding multiplier; the native test helper uses the same policy.

*Ownership and compatibility decision:* supported speed choices belong to emulation policy, while the GUI only presents the current index and submits the next one. Exposing the constant is an additive API so the separate native binary can consume the library's definition. Invalid indices still report the existing settings error, and the GUI still wraps from the final factor to the first. A constant reference adds no runtime dispatch, allocation, or copy and removes the maintenance risk of divergent arrays.

*Validation:* `cargo test --locked -p tek4051-emulator --quiet` passed all 263 tests; `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, `cargo test --locked -p tek4051-flash-drive-tests --no-run`, and `git diff --check` passed. No test source changed; the existing speed-setting and wraparound regressions exercise the shared list.

*Result and remaining work:* the speed policy now has one source in `emulation/`, with the UI and session consuming the same multipliers. Physical native speed-control interaction remains part of the unverified native acceptance checks.

== Machine ROM asset grouping (15 November 2026)

*Baseline and scope:* starting at `cfc2b81`, moved the six tracked ROM image files from package-root `roms/` to `machine/roms/`. Updated the six `include_bytes!` paths in `machine/rom.rs` and the current asset location in `migration-log.typ`. Historical entries describing the original extraction remain unchanged.

*Asset and performance decision:* the binaries are unchanged and remain compile-time embedded; only their source paths changed. `machine/rom.rs` already defines their sizes and fingerprints, and `machine/bus.rs` maps those images. The move adds no runtime work, allocation, or dependency, while making hardware ownership explicit.

*Validation:* pre-move and post-move SHA-256 digests match for all six images. `cargo test --locked -p tek4051-emulator --quiet` passed the package suite, including existing ROM size/fingerprint and mapping regressions. Formatting, strict all-target/all-feature Clippy, Flash Drive consumer compilation, all updated Typst documents, diff checks, and Rust Code Quality review passed.

*Result and remaining work:* ROM assets now reside with the machine implementation. No image data, mappings, public APIs, or runtime behavior changed.

== Native source grouping (15 November 2026)

*Baseline and scope:* continued the behavior-preserving source organization by moving native startup/composition to `native/app.rs`, the worker and image-preparation bridge to `native/backend.rs`, and their private tests and support files under `native/`. Updated the binary and test-only session `#[path]` declarations and the nested backend test paths.

*Ownership and complexity decision:* native startup, its worker adapter, and native-only fixtures now share an explicit physical group. Rust module names such as `native_backend`, sibling visibility, and the library session's test-only inclusion remain unchanged. The move adds no runtime dispatch, allocation, copy, synchronization, or dependency; source-path declarations and documentation are the only intended behavior changes.

*Validation:* `cargo test --locked -p tek4051-emulator --quiet` passed the package suite (263 tests); `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, both updated architecture Typst compilations, and `git diff --check` passed.

*Result and remaining work:* native composition and integration files are grouped under `native/` with their existing module names and private test access intact. Physical native UI acceptance remains open.

= Grade AR-5 - Integrated boundary acceptance

*Goal:* demonstrate that the new structure provides the agreed boundaries without adding unnecessary machinery or changing existing behavior.

*Work:*

- Review imports, visibility, public signatures, ownership and relevant consumers against GUI -> emulation -> machine. A rename, wrapper or passing test alone does not establish a dependency boundary.
- Decide whether Rust modules suffice or a separate machine crate provides immediate enforcement/build value. If crates are split, update actual consumers and manifests together; if they are not, record the enforced visibility and reviewed dependency limits.
- Distinguish tests that run headlessly from builds that avoid GUI dependencies entirely. The latter requires an appropriate dependency/package configuration; moving files inside a package with unconditional GUI dependencies does not achieve it.
- Remove unnecessary transitional wrappers and abstractions. For every retained addition, identify the concrete coupling, ownership ambiguity or testing obstacle it removes.
- Run the relevant complete Rust regressions and shared worker consumers, formatter, strict linter and build. Obtain focused Rust Code Quality Reviewer and Rust Test Quality Reviewer results where required, and resolve actionable findings.
- Perform bounded before/after native smoke checks for the existing Start/Pause/Reset, keyboard/panel, display, file-dialog and x100 interactions affected by the refactor. Record unavailable native evidence as a limitation or blocked check, not as passing parity.
- Update design, API/test-location guidance and handoff to describe actual structure, accepted exceptions and remaining work. Keep migration evidence and grades unchanged except for factual consumer/path updates needed by the refactor.

*Exit gate:* AR-0 through AR-4 have accepted evidence, relevant regression and native checks are satisfied or an explicit alternative has been agreed, both required review roles have no unresolved actionable findings, and the dependency/ownership contract is demonstrable. This is architectural acceptance, not migration sign-off.

= Increment, validation and evidence rules

Keep increments small and buildable. Separate mechanical moves/import updates from changes to boundary contracts where practical. Run the smallest existing test selectors covering an increment, then the relevant complete suites at a grade exit. Do not install new architectural tooling or dependencies just to certify a directory layout.

Current repository-root validation commands include:

```sh
cargo fmt --all -- --check
cargo clippy --locked -p tek4051-emulator \
  --all-targets --all-features -- -D warnings
cargo build --locked -p tek4051-emulator
cargo test --locked -p tek4051-emulator
cargo test --locked -p tek4051-flash-drive-tests \
  --test worker
```

These are implementation-validation commands, not claims that this documentation task reran them. If package boundaries change, update selectors against the actual manifests rather than preserving obsolete names. Compile changed Typst documents with the available compiler, retain the project typography and keep generated PDFs out of the repository unless requested.

For each grade, record its identifier and status, baseline revision/working state, changed scope and consumers, boundary gained, commands/results, before/after behavioral evidence, review findings, remaining risks and acceptance decision. Reference existing tests and reports instead of duplicating them into a new documentation hierarchy. A failed gate stays open; a blocked check records its cause and next action.

The sequence alone does not authorize implementation; the user's subsequent requests authorized the bounded increments recorded above. Continue with explicit ownership/interface decisions, without expanding the migration scope.
