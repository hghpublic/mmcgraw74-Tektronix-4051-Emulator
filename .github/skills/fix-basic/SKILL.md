---
name: fix-basic
description: "Diagnose and repair Tektronix 4051 numbered BASIC source files and Flash Drive import/load problems, including `.bas` metadata, byte counts, CR record endings, and message 59."
argument-hint: "BASIC source file, import/load error, or expected behavior"
---

# Fix Tektronix 4051 BASIC Files

## Purpose and Scope

Use this skill when asked to repair a Tektronix 4051 BASIC program file or diagnose why a Flash Drive `.bas` file imports but does not load or run. It covers BASIC source validation, host filename metadata, payload byte counts, and the ASCII program record format.

This skill does not authorize changing emulator behavior, rewriting BASIC statements, or modifying supplied assets outside the requested scope. If investigation points to an emulator defect rather than a malformed file, explain the evidence and handle the code change only when requested.

## Triggers

- The user asks to fix a BASIC or `.bas` file for this emulator.
- A Flash Drive file imports but `OLD@5:` reports “No program found” (message 59), or the program fails to load.
- A user reports an incorrect Flash Drive filename, file number, type/usage field, or byte-count suffix.
- The user explicitly invokes `fix-basic`.

Do not use this skill for general BASIC language help unrelated to a file repair, or for broad emulator migration and architecture work.

## Workflow

1. **Establish the writable target and preserve state.**
   - Read applicable workspace instructions and the Tektronix handoff when working in this repository.
   - Distinguish the workspace file from any attached snapshot. Attachments marked read-only are never edit targets.
   - Treat inherited files and supplied program/data assets as read-only unless the user explicitly authorizes the particular file change. A request to diagnose alone does not authorize rewriting the source.
   - Inspect the active emulator page only through read-only page snapshots or screenshots. Do not reload it, import files, clear storage, or launch a program without explicit authorization.

2. **Inspect the actual filename and payload.**
   - Record the exact path and case, file size, source lines, and raw line-ending bytes before proposing edits. Use bounded inspection such as `stat` and `od` when line-ending or byte-count evidence matters.
   - Do not assume the numeric suffix in a Flash Drive filename equals the file length. `flash_drive::parse_header` ignores that suffix during import; the actual payload bytes are stored. The shared filename formatter includes a byte count for canonical names.
   - Check whether the file number already exists: importing another entry with the same number replaces that stored entry.
   - Validate import metadata from the actual header: file number, `ASCII` type, `PROG` usage, description, and extension. Prefer the shared filename formatter over manually composing fixed-width fields.

3. **Diagnose import versus BASIC loading separately.**
   - Use the directory row, import notice, and operation status to establish whether import succeeded. Successful import does not prove that `OLD@5:` can load the payload.
   - If the visible sequence reaches `FIND@5:<number>` and then `OLD@5:` reports message 59, inspect the imported program bytes and selected file. Do not infer the cause from the error text alone; a missing/incorrect selection or malformed program records can both matter.
   - For ASCII BASIC source, verify numbered statements and CR (`0D`) record endings. The Rust editor's `basic_editor::prepare_file` validates numbered printable-ASCII lines, normalizes CR/LF source separators, and emits one CR after each retained line. A host LF byte (`0A`) is not a 4051 record terminator.
   - Preserve BASIC statements while normalizing line endings. Do not add or remove program statements, renumber lines, or alter program semantics without authorization.

4. **Apply the smallest authorized correction.**
   - Prefer the existing BASIC editor preparation and shared Flash Drive filename logic over new ad hoc formatting.
   - When correcting a text file, ensure the generated payload uses the same CR-terminated representation as `basic_editor::prepare_file`, then set the filename size suffix from the resulting byte length.
   - For byte-level CR conversion that cannot be represented safely as a normal text patch, use a narrowly scoped script with an exact expected-input check. Refuse to overwrite if the source differs from the inspected bytes.
   - Keep filename-only requests filename-only. If a necessary payload change was not authorized, report the diagnosis and ask before changing it.

5. **Verify and report.**
   - Recheck the final path, actual byte count, line-ending bytes, and that the filename's canonical size suffix matches the payload.
   - Run the smallest relevant existing test when available. The integration test `tape_runs_basic_program_through_rom` exercises imported ASCII program bytes through `FIND@5:`, `OLD@5:`, and `RUN`; report that test's scope and do not claim it verifies a different payload unless it does.
   - Do not claim a live emulator run succeeded unless the corrected file was actually re-imported and observed loading/running. State when the active page still holds an older in-memory copy.

## Constraints

- Never edit a read-only attachment snapshot or bypass its read-only status by copying changes back to it.
- Never silently replace user content, discard lines, or normalize BASIC statements. Surface validation failures and preserve the original bytes until an authorized change is ready.
- Do not clear, restore, or otherwise replace the Flash Drive directory as a troubleshooting shortcut.
- Keep any change limited to the authorized file or code scope, and do not modify unrelated assets.

## Applicable Tool Access

- Use `view`, `rg`, and `glob` for source and repository inspection; use `apply_patch` for ordinary text edits and filename changes.
- Use `bash` only for precise file metadata/byte inspection and existing focused test commands. Byte transformations must check the expected input before writing.
- Use browser page snapshots or screenshots for passive state evidence. Interactive browser actions require explicit user authorization.

## Expected Output

Report:

- The confirmed cause, distinguishing import success from BASIC load success.
- The exact workspace file changed, its final filename and payload byte count, and whether source statements were preserved.
- The validation commands and results, including any warnings or limitations.
- Whether the attachment snapshot and active browser state were left unchanged, and whether the user must re-import the corrected file.
