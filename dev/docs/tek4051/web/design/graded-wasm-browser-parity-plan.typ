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
  #text(size: 15pt)[Graded WASM Browser Parity Plan]
]

#align(center)[
  Track: browser keyboard and Flash Drive parity with the Rust application
  \
  WASM API: `dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/mod.rs`
  \
  Parity target: current Rust session and native application behavior
  \
  Status: W0-W4 complete and validated; W5 remains open; release WebGPU runtime observed
  \
  Updated: 19 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and scope

This plan adds browser input and Flash Drive application support around the existing Rust WASM session API. Its grades are ordered acceptance gates, not percentages. Passing a grade requires its observable behaviors and evidence; work in a later grade does not imply earlier parity.

The target is the current Rust emulator, not a reconstruction of every behavior of the older JavaScript application. Browser interactions may differ where browser security requires them to differ, but keyboard signals, Flash Drive contents, archive bytes, state transitions, and error outcomes must match the corresponding Rust contracts. Native path-based dialogs are replaced by browser file selection and downloads; that platform difference does not authorize different emulator or storage semantics.

Keep the machine and reusable session as the single source of emulation and tape behavior. Browser code must not obtain mutable machine state or reimplement BASIC/GPIB commands. Add no automatic browser persistence: the Rust session owns an in-memory store, and import/export are explicit. Do not add a separate delete/rename file manager. A user-authorized Clear Flash Drive action is an explicit scope extension: it clears the shared in-memory store with confirmation and does not add individual delete or rename operations.

The recommended browser shell is a small module-based web entry point adjacent to the WASM package, using the existing `wasm-bindgen` API and standard browser controls. Do not modify the legacy JavaScript reference or add a front-end framework, storage backend, or worker until a graded requirement demonstrates the need.

== Definition of parity

- *Keyboard parity* means the browser adapter delivers the same translated Tek codes, event-time modifiers, ownership, timed panel transitions, and release behavior as the Rust native adapter. Browser layout, focus, shortcuts, and text-entry mechanics are handled at the browser boundary.
- *Flash Drive parity* means the browser can observe the native directory metadata and perform the same in-session imports, archive replacement, file/archive exports, Auto Load, guarded directory-program launch, and the explicitly scoped clear-all action, with equivalent bytes, ordering, limits, state changes, and visible failures.
- *Browser equivalence* allows user-mediated file selection and downloads in place of native open/save dialogs. A page reload may discard the in-memory store just as a new Rust session does.
- Parity does not add printer, audio, screenshot export, IndexedDB persistence, or file-manager operations absent from the selected Rust application surface.

= Current baseline and gaps

#table(
  columns: (1fr, 2fr, 2fr),
  stroke: 0.5pt,
  [*Area*], [*Rust baseline*], [*Current WASM gap*],
  [Session controls], [Run/Pause, Reset, speed, display granularity, Caps Lock, advancement and immutable display/status snapshots.], [The WASM API and package-local shell expose the controls; session and browser input regressions pass.],
  [Keyboard], [The egui adapter pairs printable key events with text, maps the observed character to a Tek key, applies event-time modifiers, filters repeats/host shortcuts, retains key identity through release, and releases input on focus loss or text editing.], [The WASM mapping, browser DOM ownership/focus adapter and keyboard UI are implemented. DOM tests and selected-browser WASM input probes pass; cross-browser coverage remains open.],
  [Panel input], [The native application provides keyboard and pointer access to panel keys; the shared sequencer preserves the 100 ms minimum press/release behavior.], [The WASM session and browser panel controls use the shared sequencer; pointer lifecycle and PIA tests pass.],
  [Directory and launch], [The native view shows canonical sorted metadata, supports selection/navigation, and launches only eligible ASCII programs after the runtime is running and the user confirms the BASIC prompt. Auto Load types file-1 commands through the shared sequencer.], [The WASM API and browser directory/navigation UI expose exact metadata, host-confirmed prompt state, Auto Load, and guarded launch; interactive end-to-end evidence remains open.],
  [File workflows], [The native application imports loose files and ZIP batches, separately restores a whole-storage archive after confirmation, exports one numbered file or a whole archive, and reports cancellation/errors.], [The browser UI implements the shared workflows; selected-ZIP decode and archive encode use a stateless Rust/WASM worker, with large-operation responsiveness validated.],
  [Clear Flash Drive], [The shared session can clear the entire in-memory store and reset related tape selection/transfer state; a native clear control is not part of this plan.], [The user-authorized browser control confirms before clearing, is disabled when empty/busy, and rejects clearing during an archive-export snapshot; API and focused browser regressions pass. Real-browser release evidence remains open.],
)

== Implementation status

W0's parity inventory and selected-browser support matrix were frozen on 16
November; the separately authorized clear-all action is recorded below as a
scope extension. W1/W2's
shared key mapping, browser event ownership, focus/pointer release behavior,
controls, WASM-to-PIA regressions, and live selected-browser input evidence are
complete. W3's shared bounded ZIP decoder and typed WASM directory, import,
replacement, export, clear-store, Auto Load, readiness, launch, and notice APIs are
implemented and validated by native and no-default-feature WASM suites plus a
`wasm32-unknown-unknown` build. W4's browser picker/download/directory UI,
worker-backed selected-ZIP decoding and archive encoding, transactional restore,
confirmed Clear Flash Drive action, and large-operation responsiveness gates are
implemented and pass focused browser tests.
The 64 MiB selected import stays below the 100 ms animation-frame-gap threshold
at x1, and the 64 MiB archive export stays below it at x100; a 56 MiB whole-store
restore also stays below the threshold. W5's real-browser parity and release
evidence remain open.

BASIC prompt readiness is still explicit host confirmation, not automatic
machine detection. This plan does not treat elapsed time or the BUSY indicator
as a readiness signal.

The shared session already owns the machine, input sequencer, directory snapshot, archive streams, and import/export commands. The native adapters own filesystem reads/writes, ZIP selection, dialogs, downloads, and UI notices. Preserve this division: browser workers may perform bounded codec work but never construct or own an emulator session.

The two import workflows are distinct and must stay distinct. Loose files and ordinary ZIPs are imported as selected files into existing storage, with per-file results and shared selection limits. A Flash Drive archive is a backup/restore format that transactionally replaces the whole store after user confirmation. Exporting one numbered file uses the emulator's canonical filename and bytes; archive export uses the shared archive codec.

= Grade W0 - Freeze the Rust parity contract

*Goal:* enumerate the exact Rust behavior that the browser app must reproduce before changing the WASM boundary.

*Work:*

- Build a keyboard matrix from `gui/keyboard_input.rs` and its private tests: printable text pairing, all mapped ASCII characters and shift inversions, supported function/navigation keys, event-time Shift/Control, repeat and host-shortcut filtering, key identity, text-edit ownership, focus loss, and pointer cancellation.
- Build a Flash Drive matrix from `gui/app.rs`, `gui/flash_directory.rs`, `emulation/session.rs`, `host/import.rs`, and `host/io.rs`: directory rows/invalid metadata, loose and ZIP batch import, archive replacement confirmation, numbered-file export, archive export, notices/cancellation, Auto Load, prompt readiness, launch eligibility, and launch cancellation.
- Mark the native-visible operations that do not exist; do not infer new delete, rename, persistence, or host-path requirements.
- Fix representative byte fixtures, filenames, metadata rows, import order, malformed inputs, error messages, and archive-boundary cases from existing Rust tests. Record which outcomes are public contracts and which are presentation-only.
- Select the initial browser support matrix and package-local entry point. Recommended default: plain HTML/JavaScript modules, no persistence beyond the session, browser file picker for imports, and downloads for exports.

*Frozen evidence inventory:*

#table(
  columns: (1.1fr, 2.2fr, 2.4fr),
  stroke: 0.5pt,
  [*Keyboard contract*], [*Frozen inputs and outcomes*], [*Evidence*],
  [Printable text], [All printable ASCII characters 0x20-0x7e; explicit shifted aliases include `#`, `'`, `+`, `@`, and `~`. Control bytes, DEL, NUL, and non-ASCII text are unmapped.], [`tests/keyboard_mapping.rs::printable_text_matches_reference_character_mapping`; `gui/keyboard_input/tests.rs::text_events_map_shifted_glyphs_before_machine_input`.],
  [Special keys], [Backspace, Tab, Enter, Escape, Delete, Insert, Home, End, PageUp, F1-F10, and F13-F17 map to the listed Tek codes; unsupported arrows and F11, F12, F18, and F35 remain unmapped.], [`tests/keyboard_mapping.rs::logical_panel_keys_match_reference_codes`; `tests/wasm_api.rs::browser_key_mapping_matches_shared_keyboard_rules`.],
  [Event ownership], [Printable keys pair with same-event text; Shift/Control are sampled at event time; repeats and host shortcuts are filtered; focus/visibility loss releases keys; pointer cancel/lost capture releases panel keys.], [`platforms/wasm/browser-tests/input.spec.mjs` covers text/punctuation pairing, repeat/modifier handling, shortcuts/text controls, focus/visibility loss, and pointer cancellation; `gui/keyboard_input/tests.rs` covers native PIA and ownership behavior.],
  [WASM outcome], [A paired `@` maps to Tek `0x40`, inverts the held Shift line, reaches the keyboard PIA, and focus loss returns the key port to `0x80`. The Back Space panel press reaches the PIA as `0xf1`.], [`platforms/wasm/tests.rs::browser_text_mapping_reaches_keyboard_pia`; `platforms/wasm/tests.rs::browser_panel_press_reaches_keyboard_pia`; live generated-WASM adapter probe in the selected browser.],
)

#table(
  columns: (1.1fr, 2.2fr, 2.4fr),
  stroke: 0.5pt,
  [*Flash Drive contract*], [*Frozen fixtures and outcomes*], [*Evidence*],
  [Directory], [Shuffled files 254 LAST, 4 ASCII/TEXT, 1 BINARY/DATA, 7 NEW, 3 ASCII/PROG, and 6 BINARY/LOG sort numerically. Metadata retains optional type/usage, exact byte lengths, Unicode descriptions, and a split surrogate; the GUI reports invalid descriptions rather than substituting text.], [`tests/gpib.rs::flash_directory_lists_sorted_canonical_metadata_and_raw_descriptions`; `native/tests.rs::flash_directory_panel_reports_invalid_description_while_stopped`.],
  [Loose and selected ZIP imports], [Imports preserve selection/member order and exact bytes, ignore directory entries, and keep valid entries when a sibling filename or ZIP member fails. A nested-path fixture contains `10 END\\r\\r` and binary `[0, 0xff]`.], [`native/tests.rs::native_flash_archive_reader_imports_entries_and_skips_directories`; `native/tests.rs::native_flash_archive_reader_reports_corrupt_members_and_keeps_good_files`; `tests/gpib.rs::flash_import_keeps_successes_reports_failures_and_selects_first_stored_file`.],
  [Selected ZIP bounds], [Compressed input is capped at 128 MiB; entry count at 4096; each expanded entry at 8 MiB; selected ZIP expansion at 64 MiB. Exact limits are accepted where the parser can reach them; one-over inputs are rejected with bounded reads and no budget spending on compressed-size rejection.], [`native/tests.rs::native_flash_archive_reader_bounds_compressed_file_size_before_parsing`; `native/tests.rs::native_flash_archive_reader_bounds_entry_expansion`; `native/tests.rs::native_flash_zip_selection_shares_total_and_entry_budgets`; `native/tests.rs::native_flash_archive_reader_enforces_entry_count_boundary`.],
  [Whole-store archive], [Archives alternate ordered JSON metadata/content entries. Fixtures cover Unicode, quotes, slash, backslash, NUL, an unpaired surrogate, duplicate numbers (last pair wins), and empty storage. Odd entry counts or malformed headers fail atomically and retain prior files/transfer snapshots.], [`tests/gpib.rs::flash_storage_archive_writes_web_compatible_ordered_pairs`; `tests/gpib.rs::flash_storage_archive_import_replaces_store_and_resets_transfer_snapshot`; `tests/gpib.rs::flash_storage_archive_import_is_atomic_on_malformed_input`; `tests/gpib.rs::flash_storage_archive_import_accepts_empty_archive_and_clears_old_snapshot`.],
  [Exports and launch], [Numbered export returns canonical names and exact bytes; invalid numbers, missing files, and unrepresentable names fail explicitly. Auto Load and directory launch require a running session; launch additionally requires explicit BASIC-prompt confirmation and an ASCII PROG file.], [`tests/gpib.rs::flash_export_round_trips_bytes_and_representable_metadata`; `tests/gpib.rs::flash_export_rejects_missing_numbers_and_unrepresentable_names`; `native/tests.rs::auto_load_control_requires_running_and_reports_interruptions`; `native/tests.rs::flash_directory_keyboard_selection_launches_program_through_rom`.],
)

The browser contract is one selected matrix: the VS Code integrated browser on macOS (`Code/1.146.0`, Electron 43.8.6, Chromium 150.0.7871.250, `en-GB`). This is not a claim of cross-browser coverage. Browser file selection and downloads replace native path dialogs; error visibility, store contents, metadata, ordering, limits, and bytes remain parity requirements. Dialog wording and native path text are presentation details.

*Exit gate:* each in-scope behavior maps to an existing Rust contract/test or a clearly identified missing Rust baseline. The browser-specific substitutes and explicit non-goals are recorded; no WASM or machine behavior changes are included in this grade.

= Grade W1 - Share keyboard translation rules

*Goal:* prevent browser and native key translation from drifting.

*Work:*

- Separate the pure mapping rules from egui event interpretation. Reuse the framework-neutral input types (`InputFrame`, `InputAction`, `KeyModifiers`, and `ManualKeyId`) rather than creating another input protocol.
- Keep egui-specific key identity and event extraction in the native adapter. Let the browser adapter supply logical key/text events to the shared mapping rules without adding egui to the WASM feature graph.
- Preserve the current text-event precedence, ASCII acceptance, character-to-Tek mapping, selected Shift-line inversions, and filtering of unsupported/shortcut events. Do not "correct" a Rust mapping as incidental parity work.
- Retain or extend the native keyboard tests so the extraction proves unchanged mappings and event ownership.

*Exit gate:* native keyboard tests pass unchanged in behavior; table-driven shared tests cover every supported text mapping and special-key result; the WASM-only dependency graph remains free of eframe/egui.

= Grade W2 - Browser keyboard, focus, and panel input

*Goal:* connect browser events and front-panel controls to the shared session without stuck keys or leaked UI input.

*Work:*

- Add a browser adapter that uses stable key identities for down/up, applies modifiers at event time, pairs a printable key with the corresponding text input, suppresses repeats and host shortcuts consistently, and forwards only supported Tek input.
- Send modifier-only changes through the existing API. Call `release_input()` on surface blur and document visibility loss; also clear any browser-side ownership map so a missing key-up cannot latch a machine key.
- Do not route key events from focused browser controls to BASIC. Prevent browser defaults only for keys the emulator owns; preserve browser/application shortcuts and text-field editing.
- Add browser pointer controls for panel keys. Pair pointer down/up, cancel, and lost-capture paths; preserve the shared sequencer's timing and release-time modifiers.
- Connect Caps Lock, Start/Pause, Reset, speed, and display controls to the existing session API. Preserve the running state and UI focus/ownership behavior established by the native app.

*Exit gate:* browser event tests cover representative letters, punctuation/shift inversions, function keys, press/release identity, repeated keys, modifier-only changes, focus loss, text editing, and pointer cancellation. The same key and PIA outcomes are observed through the WASM session; native GUI keyboard tests still pass.

= Grade W3 - Expose the Flash Drive session contract

*Goal:* make directory state and storage operations available to a browser host without leaking the machine or pretending browser I/O is a filesystem path.

*Work:*

- Extend immutable browser snapshots with the canonical directory metadata required by the Rust view: number, type, usage, byte length, and description. Preserve invalid UTF-16 and missing metadata as explicit states; do not silently substitute lossy strings.
- Add typed browser operations for importing loose files, importing a selected-file batch, replacing storage from a Flash Drive archive, exporting one numbered file, exporting an archive, Auto Load, prompt readiness, and launching a selected file.
- Add the explicitly authorized clear-all operation through the shared session. It resets stored files and related tape selection/transfer state, and rejects mutation while an archive-export snapshot is active.
- Represent host work as explicit typed results/effects containing the browser-relevant name and owned bytes. Replace the current generic "unsupported host file effect" failure only for operations implemented by the browser host; keep every unsupported effect explicit.
- Reuse `EmulationState`, machine import/export methods, canonical filename generation, archive streams, notices, and the existing transactional replacement semantics. Keep the session as the exclusive owner of storage state.
- Make the bounded ZIP-selection decoder usable by both native and browser hosts without duplicating parsing or relaxing entry, compressed-size, per-entry, or total expanded-byte limits. Keep path access in the native host adapter.

*Exit gate:* WASM-feature tests verify directory metadata and invalid descriptions, import order and partial errors, corrupt-archive rollback, full-store replacement and clear confirmation boundaries, transfer-state reset, archive-snapshot mutation rejection, exact single-file bytes/name, exact archive round-trip, launch gating, and visible failure outcomes. Native import/export tests remain green.

= Grade W4 - Implement browser file selection and downloads

*Goal:* provide the browser equivalents of the native Flash Drive file dialogs and directory UI.

*Work:*

- Import loose files and ordinary ZIP batches from a user-selected file list. Convert filenames and bytes once, preserve selected order, apply shared budgets before unbounded reads, and show each success or error.
- Provide a separate Flash Drive archive restore action with an explicit replacement confirmation. Cancellation leaves the current directory unchanged; failed parsing never commits a partial replacement.
- Export a selected numbered file with the exact canonical name and bytes as a browser download. Export/restore the whole-storage archive using the shared ZIP format; revoke temporary object URLs and surface download/encoding failures.
- Render the Rust-equivalent directory metadata and navigation. Preserve selection/focus, Up/Down plus Enter and double-click launch, the running-session and BASIC-prompt confirmation gates, ASCII-program eligibility, Auto Load, and cancellation/status notices.
- Add the confirmed Clear Flash Drive control. Keep it unavailable when storage is empty or the host operation is busy; cancellation preserves files, and confirmation clears all files and updates the empty-directory state.
- Preserve session snapshot/freeze ordering around asynchronous host operations so exports refer to the acknowledged store state. Do not block the repaint/input loop with file reads, ZIP work, or archive encoding.
- Measure normal and maximum-allowed archive operations and sustained x1/x100 emulation. Add a Web Worker only if the measured main-thread work violates the responsiveness gate; avoid introducing a second session owner or copying framebuffer/storage payloads without evidence.

*Exit gate:* browser tests import one and multiple files, import ordinary ZIPs, cancel and confirm archive replacement, restore/export the whole archive byte-for-byte, export a numbered file byte-for-byte with its canonical name, launch a supported program only after prompt confirmation, and verify errors and cancellation preserve state. They also verify Clear Flash Drive empty/busy gating, cancel/confirm behavior and empty-state update. The API rejects clear during an archive-export snapshot. The browser remains responsive during the largest allowed input in the selected test set.

= Grade W5 - End-to-end parity and release evidence

*Goal:* show the browser app is an operational host for the same Rust session behaviors, not only a compiling WASM library.

*Work:*

- Run the existing native package tests and all new WASM API/browser tests; build `wasm32-unknown-unknown` with the locked dependency graph and no native feature.
- Exercise keyboard and Flash Drive workflows in the selected real browser: focus entry/exit, basic command typing, front-panel press/release, import, directory inspection, guarded launch, export, archive restore, Clear Flash Drive, and cancellation/failure. Verify that clearing resets the visible directory and does not interfere with an active archive snapshot.
- Compare captured bytes, directory metadata, key actions, and session notices against the Grade W0 Rust fixtures. Record browser/OS versions and any explicitly accepted presentation differences.
- Check long tasks, frame/input responsiveness at supported speeds, memory growth during bounded imports, and pixel-copy frequency. Compare snapshot revisions before copying the framebuffer.
- Update this plan's grade status only from recorded evidence. Keep keyboard, Flash Drive, and overall parity status independent if one area is incomplete.

*Exit gate:* every Grade W0 row passes in the supported browser matrix; native regressions pass; errors are visible; input cannot remain latched; file/archive bytes and limits match Rust; performance and any browser-only differences are documented. Only then claim WASM keyboard and Flash Drive parity.

= Complexity and compatibility constraints

- Prefer the shared session and existing neutral input contracts over duplicate emulation, duplicate filename/metadata codecs, or a second storage model.
- Do not add IndexedDB, a framework, a generic filesystem abstraction, a worker, or a new crate by default. Each needs a demonstrated product requirement or measured performance/dependency benefit.
- Keep browser-only APIs at the browser host boundary. The machine and shared codecs must remain usable without DOM types.
- Avoid cloning the full directory payload for display. Expose metadata separately; copy file/archive bytes only when a browser transfer requires owned bytes.
- Preserve the native worker's acknowledged snapshot/freeze invariant for imports and exports. Browser asynchronous UI must not allow a half-read import to mutate storage or an export to mix directory metadata from one state with bytes from another.
- Treat user cancellation as cancellation, not success; surface parse, size, encoding, export, and runtime failures explicitly.
- Browser file pickers cannot select arbitrary host paths and downloads do not provide an arbitrary Save As path. These are documented platform differences, not parity failures.

= Requirements and grade mapping

#table(
  columns: (0.7fr, 1.7fr, 2.8fr),
  stroke: 0.5pt,
  [*ID*], [*Required outcome*], [*Grades / evidence*],
  [KEY-01], [Browser-translated keyboard matches Rust key mapping, modifiers, ownership, filtering, and focus-loss release.], [W0-W2 mapping tables, shared/native tests, and browser event regressions.],
  [KEY-02], [Front-panel input and browser UI controls preserve Rust timing and do not leak UI-owned keys to BASIC.], [W0, W2, W5 panel, focus, launch and responsiveness tests.],
  [FD-01], [Browser sees Rust-equivalent canonical directory metadata and launch eligibility.], [W0, W3, W4 directory snapshot and launch-gate tests.],
  [FD-02], [Loose and ZIP file imports preserve Rust order, validation, limits, and error reporting.], [W0, W3, W4 bounded-import and state-preservation tests.],
  [FD-03], [Whole-store archive restore/export and numbered-file export preserve exact Rust bytes, naming, and transactional behavior.], [W0, W3-W5 round-trip and browser download tests.],
  [FD-04], [Auto Load, readiness confirmation, cancellation, and host failures match native session behavior.], [W0, W3-W5 sequence, prompt, and failure tests.],
  [FD-05], [The scoped clear-all action confirms intent, resets the in-memory directory/tape state, and cannot mutate an active archive snapshot.], [W3-W5 clear API, state-reset, busy/cancel/confirm, and real-browser evidence.],
  [PAR-01], [Browser parity passes without changing machine semantics or introducing unmeasured responsiveness regressions.], [W0, W5 native/WASM tests, browser evidence, and performance record.],
)

== Status

W0's browser matrix and original parity inventory were frozen on 16 November; the Clear Flash Drive workflow is a separately authorized scope extension dated 17 November. W1/W2 shared keyboard mapping, browser event ownership, focus/pointer release behavior, controls, WASM-to-PIA tests, and selected-browser input evidence pass. W3's shared Flash Drive APIs, including clear-state reset and the active-snapshot guard, pass native and no-default-feature WASM regression suites. W4's package-local browser workflows, worker-backed selected-ZIP decode/archive encoding, transactional restore, exact downloads, confirmed clear control, and large-operation responsiveness tests pass. W5's cross-browser/manual workflow evidence and release record remain open; overall keyboard and Flash Drive parity is not signed off.

The `wasm` feature is paired with the `platforms/wasm/browser/` shell, while the existing native Rust GUI, tests, and shared session remain the behavior baseline; the legacy web reference may corroborate mappings but does not expand this plan's scope.

The optimized release WASM was also run in the integrated browser with WebGPU
selected and BASIC prompt readiness detected at x20. The separate
#link("./wasm-performance-optimization-report.typ")[WASM performance optimization report]
contains host-qualified CPU-fallback measurements and build-profile
comparisons. This runtime and performance evidence does not close W5's
cross-browser/manual workflow requirements or sign off overall parity.
