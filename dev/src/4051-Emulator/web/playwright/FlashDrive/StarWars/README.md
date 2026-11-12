# StarWars browser autoplay

Runs the unchanged [StarWars assets](../../../FlashDrive/StarWars) in the web
4051 reference, using the root npm workspace and shared Playwright harness.

From the repository root:

```sh
npm run test:starwars
npm run test:starwars:ui
```

`npm test` includes this workspace. Dependencies and the lockfile live only at
the repository root; no per-suite installation or browser download is needed
when the shared dependencies and Chromium are already available.

## Validation strategy and coverage

The browser is the primary validation tier: the real emulator ROM executes the
original BASIC, with real Storage-dialog import and in-memory tape I/O. There
is no database, container tier, authentication, or backend build. Every test
imports a fresh tape in a fresh page. Shared hooks retain uncaught errors,
unexpected dialogs, and non-consuming tape-operation observations.

The 11 files comprise seven ASCII programs, three binary numeric-vector files
(4, 5, 6), and LAST marker 124. Filename lengths are retained verbatim.

- Import all files with their original numbers; verify SHA-256 integrity of
  all three binary files.
- Run generators 1, 12, 13, and 14; verify complete numeric-array writes to
  files 5/6, then use their original `RUN 3000` readback/drawing entry points.
- Select each original R12 game fighter, read its real data file, show
  instructions, and turn in wings using rank zero.
- Start the R12 game with a cockpit, recall instructions with UDK16, and end
  with rank zero. UDK16 is Shift+UDK6, not scancode 0x6F. Graphics readiness
  requires the completed cockpit's left border and excludes the PAGE erase flash.
- Run TLIST through file 124 and press RETURN. Despite its prompt mentioning a
  root main menu, the original code loads and runs file 1, a TIE generator.

The separate 4054 Option 30 game (file 3) is explicitly skipped because it uses
`RINIT`, `ROPEN`, and `VISIBILITY`, not emulated 4051/R12 interfaces. Generators
12 and 13 are not skipped merely because their filenames say Opt30: their
refresh statements are comments and their normal drawing uses 4051 commands.
File 12's comparison entry renders only fragments of the new-design fighter;
the tests preserve that output rather than repairing its drawing routine.

File 14's alternate `RUN 5000` stick-figure entry was executed and stopped with
dimension/subscript error 10 at BASIC line 5080, before any write to file 6.
Two source lines numbered 5020 cause `C=0` to replace `DIM C(16*3)`. This entry
is a separate explicit known-defect skip; the imported ROTJ file 6 remains usable
by the game. The default file 14 generator and its comparison entry still run.

The R12 game is supplied as WIP. These scenarios do not certify complete
gameplay, aiming, scoring, explosions, sound, or all alternate BASIC entries.
No BASIC edits or compatibility patches are made.

Screenshots are 3200 x 2400. Screenshots, tape traces, vector-write counts, and
runner output under `test-results/` / `playwright-report/` are intentionally
Git-ignored, as are root dependencies and cache. Automation source is not ignored.

## Recorded validation

On 13 November 2026, repository-root `npm test` exited 0: StarWars passed 9
scenarios with 2 explicit skips; all four workspaces totalled 26 passed and
6 skipped. The previous three-workspace baseline was 17 passed / 4 skipped.
Syntax checks passed for the new configuration and test modules. Screenshots
were visually checked for the complete cockpit, recalled instruction page,
vector comparison, and directory. No protected reference assets were changed.
