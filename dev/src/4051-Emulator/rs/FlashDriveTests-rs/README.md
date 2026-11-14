# Visible Rust Flash Drive tests

On macOS, use the application-bundle launch below for the visible test. Direct
Cargo launch remains available, but did not display a window in the user's
environment. Run its core regression from the repository root:

```sh
cargo test --locked -p tek4051-flash-drive-tests --features interactive --test simple_demo
```

The explicit feature prevents ordinary workspace tests from opening a window.
The test has no libtest harness: its `main` runs eframe on macOS's main thread.
A graphical desktop is required.

## Additional supplied-program smoke targets

The manifest registers ten additional harnessless, `interactive`-gated targets:

| Target | Unchanged asset directory | Selected-entry startup check |
| --- | --- | --- |
| `battlestar` | [Battlestar](../../web/FlashDrive/Battlestar/) | Changed stored screen, more than 500 stored pixels and I/O lit (BUSY may remain lit) |
| `star_wars` | [StarWars](../../web/FlashDrive/StarWars/) | Changed drawing below row 200, more than 500 stored pixels, BUSY/I/O dark |
| `benchmark` | [Benchmark](../../web/FlashDrive/Benchmark/) | Changed stored screen, more than 500 stored pixels and I/O lit (BUSY may remain lit) |
| `asteroids` | [Asteroids](../../web/FlashDrive/Asteroids/) | At least three different stored drawing masks while BUSY is lit and I/O dark |
| `f35` | [F35](../../web/FlashDrive/F35/) | File 200: changed drawing below row 200, more than 500 stored pixels, BUSY/I/O dark |
| `fractals` | [Fractals](../../web/FlashDrive/Fractals/) | File 1: initial sides-input screen using the shared input check |
| `pictures` | [Pictures](../../web/FlashDrive/Pictures/) | File 1: initial picture-menu input screen using the shared input check |
| `svg2fg` | [SVG2FG](../../web/FlashDrive/SVG2FG/) | File 1: initial single/all-files selection using the shared input check |
| `spacelab` | [Spacelab](../../web/FlashDrive/Spacelab/) | File 1: first input screen after the viewer's display loop |
| `michlanglo` | [Michlanglo](../../web/FlashDrive/Michlanglo/) | File 1: initial filled/unfilled polygon question using the shared input check |

Each displays boot, FIND for its selected entry, `OLD@5:`, and `RUN`, then requests automatic
close five seconds after its result. All files in each new directory are
imported in sorted filename order, excluding only `.DS_Store`; errors are
reported rather than silently dropping assets. Asteroids contains two file-81
variants; the later sorted filename replaces the earlier numbered entry under
the existing import policy.

F35 deliberately uses the user-selected **file-200 BMP DISPLAY** program and
`FIND@5:200`, because file 1 is binary drawing data. It runs the original
22-file drawing loop without asking or supplying an answer. All other smoke
targets use `FIND@5:1`. Spacelab's original viewer changes to ROOT and reads
file 123, which is absent from its imported directory; directory support and
that external data dependency remain compatibility limitations, not patched
or silently supplied fixtures.

These are **startup/activity smoke checks**, not complete gameplay, exact
screen recognition, generic BASIC-error detection, or benchmark timing
certification. Battlestar stops at its initial input screen; Benchmark does not
answer its questions or run the selected benchmark. StarWars file 1 is a
Tie Fighter drawing/data-creation program, not the other supplied game
variants. Asteroids deliberately uses the user-selected **file-1 4054 Option
30 program**, not file 2's 4051 variant. Unsupported execution or missing
readiness/activity remains a genuine nonzero failure; no emulator commands or
supplied assets are modified to make these tests pass.

The input checks use the whole stored screen; drawing checks exclude rows
0-199 and transient intensities, as SimpleDemo does. Screen/drawing fingerprints
must differ from the pre-RUN frame. Drawing checks wait at least one logical
second, and all readiness checks remain true for 250 ms. These predicates alone
cannot certify full program correctness.

The window is a read-only scripted view, not a manual BASIC terminal. Reaching
Benchmark's first question completes its startup smoke test; it should not wait
for the user to type an answer.

### Verified outcomes on 14 November 2026

- StarWars and SimpleDemo: the user manually confirmed visible windows and PASS.
- Benchmark: its first prompt initially stalled because the smoke predicate
  incorrectly required BUSY dark. The real-ROM regression now verifies that
  INPUT retains both BUSY and I/O and completes successfully. After rebuilding,
  the user confirmed the window closes shortly after displaying the question;
  they did not explicitly report observing the PASS label.
- Battlestar: the user reported a syntax error at line 3490, `ROPEN 100`.
- Asteroids: the user reported a syntax error at line 4, `R=R+1` (the supplied
  line also contains `MOD 16`). These compatibility failures are retained,
  without rewriting assets or substituting a different entry program.

All five bundles compile and have validated unique app identities. Thirteen
focused test executions pass, including real-ROM Benchmark/StarWars completion
and private smoke-guard regressions (the shared private tests run in both
windowless test crates). Both required Rust reviews have no remaining findings.
Animation regressions reject pre-RUN masks, A/B/A repetitions, interrupted
qualification, stale/undersized/erased screens and wrong lamps.

Six-set follow-up:

- F35: the user confirmed visible PASS for the selected file-200 drawing loop.
  Its windowless real-ROM regression also completes FIND/OLD/RUN and the
  drawing-completion check, rather than checking only the configured filename.
- Fractals, Pictures, SVG2FG and Michlanglo: the user confirmed a visible
  startup question/menu followed by automatic close.
- Spacelab: the user reported undefined variable at line 250, message 36.
  This compatibility failure is retained with the original ROOT/file-123
  dependency, without rewriting the program or supplying substitute data.

All six new bundles build and have validated unique identities. Thirteen
focused test executions pass, including completed real-ROM FIND commands for
all smoke sets and full Benchmark/StarWars/F35 startup execution. Native
presentation is manually verified, not screenshot-certified.

For example, direct Cargo compilation/execution is:

```sh
cargo test --locked -p tek4051-flash-drive-tests --features interactive --test battlestar
```

Substitute `star_wars`, `benchmark`, or `asteroids` for the other targets. For a
visible macOS session, use the corresponding application-bundle launch below;
an exit-0 core result does not establish window visibility.

The same command accepts `f35`, `fractals`, `pictures`, `svg2fg`, `spacelab`,
and `michlanglo`. These six targets reuse the existing smoke guards rather than
adding full program-interaction scripts. Initial menu/question checks do not
certify fractal generation, picture/SVG rendering, or polygon fill correctness.

## macOS application launch

The test explicitly requests a regular foreground macOS application and an
active, visible viewport. These process-local settings do not change account
settings. They did not resolve the reported missing window on their own.

For the user-approved application-bundle launch path, build from the repository
root (Rust/Cargo and the workspace's Node.js installation are required):

```sh
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh
```

The optional argument selects any registered graphical target; omission
preserves SimpleDemo:

```sh
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh battlestar
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh star_wars
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh benchmark
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh asteroids
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh f35
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh fractals
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh pictures
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh svg2fg
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh spacelab
sh dev/src/4051-Emulator/rs/FlashDriveTests-rs/scripts/package-macos-app.sh michlanglo
```

Open the corresponding `target/macos/Tek4051 <name> Test.app` manually in Finder
(`SimpleDemo`, `Battlestar`, `StarWars`, `Benchmark`, `Asteroids`, `F35`,
`Fractals`, `Pictures`, `SVG2FG`, `Spacelab`, or `Michlanglo`). Each bundle
has its own application identity and explicitly disables agent/background-only
mode. It runs the same compiled graphical test and still reads assets from this
workspace; it is not a portable redistribution. Build output under `target/`
is Git-ignored. The script builds/packages only, without opening or focusing
applications. Window visibility must be confirmed manually while desktop
inspection remains blocked; a core exit-0 result alone is insufficient.

The shared key sequencer also has a normal, windowless regression:

```sh
cargo test --locked -p tek4051-flash-drive-tests --test keyboard
```

It checks repeated characters and RETURN at their exact press/release times.

The UI-independent [session module](tests/support/session.rs) owns asset import,
ROM stepping, stage transitions, input timing, readiness checks, and terminal
outcomes. The [shared window adapter](tests/support/visible.rs) owns framebuffer upload,
the live view and automatic-close requests. The [session worker](tests/support/worker.rs)
owns stepping and the wall deadline independently of native repaints. Both it and
the [windowless session tests](tests/session.rs) use the same driver:

```sh
cargo test --locked -p tek4051-flash-drive-tests --test keyboard --test session --test worker
```

The window displays the real Rust machine framebuffer and the live scripted
command sequence. This is a test-specific eframe view of the same emulator core,
not automation of the production application's buttons or native file chooser.
It reads all seven original files from
[SimpleDemo](../../web/FlashDrive/SimpleDemo/), without editing them:

1. Wait for ROM command readiness.
2. Type `FIND@5:1`, wait for BUSY to clear.
3. Type `OLD@5:`, wait for BUSY to clear.
4. Type `RUN`, wait for Mickey Mouse and the I/O input lamp.
5. Answer `N`, wait for different Death Star graphics and I/O.
6. Answer `N`, wait for Instant Art's cleared picture area and input.
7. Answer `TEK4051`, wait for generated artwork and I/O.
8. Answer `Q`, verify the original Mickey Mouse picture returns.

Keys have 100 ms presses and releases. Execution waits use x100 speed; typing
uses x1. A worker advances deterministic 10 ms emulated ticks, with at least
10 ms between tick starts; expensive ROM work may take longer. Repaints only
read the latest consistent framebuffer/status snapshot. Slow native frames no
longer starve emulation or its deadline. This is not a real-time calibration test.

Checks require readiness to remain true for 250 ms, use the existing stored
pixel intensities, and compare picture masks below row 200 (excluding prompt
echo and transient cursors). The 500-pixel activity boundary follows the web
SimpleDemo tests; this is not an exact golden-image comparison or generic BASIC
error OCR. Each stage has a 30-second emulated deadline and the whole test has
a 180-second wall-clock deadline.

Boot readiness uses the reference's BUSY-dark condition after a minimum
startup interval; it does not require repeated samples of one CPU instruction
address. Desktop/window-service failures can still delay presentation; timeout
diagnostics include the current stage, emulated elapsed time, PC, and lamps.

Pass or failure freezes the result and requests automatic close after five
seconds. Actual closure depends on the native event loop; this is not a
process-level watchdog when desktop services block.
Failures, timeouts, native-window errors, and closing before completion return
a nonzero exit status. Missing emulator commands are **not** skipped or accepted
as success. The current Rust implementation may fail this regression.

The worker tests leave the view completely unread for 15 seconds and require
boot progress without any GUI callbacks. They also verify that a terminal
failure and its framebuffer remain readable after the worker is joined.
Worker startup, publication, and panic failures are surfaced; every graphical
exit path cancels and joins the worker before reporting a result.

## Latest execution

The independent-worker graphical run on 14 November 2026 **passed all eight
stages and exited 0**, including Mickey Mouse, the changed Death Star picture,
Instant Art input/artwork, and the matching returned Mickey mask after `Q`.
The user subsequently reported that **no window was displayed** from direct
Cargo launch, including from their own Terminal and after explicit foreground
startup settings. Therefore, those exit-0 runs confirm only the ROM/framebuffer
predicates. Native XPC warnings remained; the exact direct-launch cause is not
established.

The packaged macOS app was then built and opened manually in Finder. The user
confirmed **a visible window with the live command sequence and final PASS**.
This satisfies the visible-session requirement through the application-bundle
launch path, not through the still-unverified direct Cargo path. Desktop
inspection remains blocked, and no screenshot or exact visual-parity
certification was obtained.

All six keyboard/session/worker tests, strict all-target/all-feature package
Clippy, formatter, and workspace test build passed. The additional private
snapshot regression checks changed pixels/revision and a newly terminal result
replacing previously published state. Both required focused reviews have no
remaining findings.

The retained native-startup code review found no bugs. Its focused test review
highlighted that exit 0 alone cannot establish physical visibility; the explicit
manual app-launch check above addresses that verification requirement.

### Previous execution, before independent stepping

On 14 November 2026, the corrected graphical test imported all seven files but
timed out at boot: only 280 ms of emulated stage time advanced during the
180-second wall deadline (PC `C8AE`, BUSY/I/O dark). macOS repeatedly reported
XPC connection errors. The result/automatic-close path returned exit status 1;
no BASIC command was reached, so this is not evidence of missing commands.
The exact cause of the slow frame updates is unresolved. Desktop inspection was
sandbox-blocked, and execution continued without screenshots by agreement.

This earlier timeout is superseded by the successful independent-worker run
above. No emulator commands, supplied programs, dependency versions, or
accessibility settings were changed to obtain that result.
