//! UI-independent Flash Drive sessions: import original assets, drive timed
//! ROM keyboard input, and check each scenario's lamps and stored graphics.
//! Callers advance fixed logical ticks and supply the wall-deadline trigger.
//! A terminal outcome freezes emulation; presentation and window closing remain
//! the caller's responsibility.

use std::{fs, path::Path, time::Duration};

#[path = "keyboard.rs"]
mod keyboard;
use keyboard::{KeyEvent, KeySequence};
use tek4051_emulator::{
    emulation::CpuReferenceRenderer,
    host_runtime::HostRuntime,
    machine::{Machine, MachineIndicators, gpib::FlashDriveFile},
};

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

pub(super) const TICK: Duration = Duration::from_millis(10);
const STAGE_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const TOTAL_TIMEOUT: Duration = Duration::from_secs(180);
const FILES: [&str; 7] = [
    "1      ASCII   PROG Simple Demo     585",
    "2      ASCII   PROG Instant Art      1757",
    "3      ASCII   PROG Simple create    1243",
    "9      BINARY  DATA Death Star       10088",
    "23     BINARY  DATA Mickey Mouse     932",
    "119    ASCII   PROG TLIST            339",
    "124    LAST                          0",
];

#[derive(Clone, Copy)]
enum Check {
    Boot,
    Idle,
    Mickey,
    DeathStar,
    ArtPrompt,
    Artwork,
    ReturnedMickey,
    InputScreen,
    FinishedDrawing,
    AnimatedDrawing,
}

#[derive(Clone, Copy)]
pub(super) struct Step {
    pub(super) label: &'static str,
    pub(super) keys: &'static [u8],
    check: Check,
}

pub(super) const STEPS: [Step; 8] = [
    Step {
        label: "Wait for ROM command readiness",
        keys: b"",
        check: Check::Boot,
    },
    Step {
        label: "Find program 1",
        keys: b"FIND@5:1\r",
        check: Check::Idle,
    },
    Step {
        label: "Load Simple Demo",
        keys: b"OLD@5:\r",
        check: Check::Idle,
    },
    Step {
        label: "Run and observe Mickey Mouse",
        keys: b"RUN\r",
        check: Check::Mickey,
    },
    Step {
        label: "Answer N; observe Death Star",
        keys: b"N\r",
        check: Check::DeathStar,
    },
    Step {
        label: "Answer N; wait for Instant Art input",
        keys: b"N\r",
        check: Check::ArtPrompt,
    },
    Step {
        label: "Answer TEK4051; observe generated artwork",
        keys: b"TEK4051\r",
        check: Check::Artwork,
    },
    Step {
        label: "Answer Q; return to Mickey Mouse",
        keys: b"Q\r",
        check: Check::ReturnedMickey,
    },
];

const LOAD_STEPS: [Step; 3] = [
    STEPS[0],
    STEPS[1],
    Step {
        label: "Load entry program from file 1",
        keys: b"OLD@5:\r",
        check: Check::Idle,
    },
];
const INPUT_STEPS: [Step; 4] = [
    LOAD_STEPS[0],
    LOAD_STEPS[1],
    LOAD_STEPS[2],
    Step {
        label: "Run; observe the initial input screen",
        keys: b"RUN\r",
        check: Check::InputScreen,
    },
];
const DRAWING_STEPS: [Step; 4] = [
    LOAD_STEPS[0],
    LOAD_STEPS[1],
    LOAD_STEPS[2],
    Step {
        label: "Run; observe drawing and return to command readiness",
        keys: b"RUN\r",
        check: Check::FinishedDrawing,
    },
];
const ANIMATION_STEPS: [Step; 4] = [
    LOAD_STEPS[0],
    LOAD_STEPS[1],
    LOAD_STEPS[2],
    Step {
        label: "Run 4054 Option 30 program; observe running graphics activity",
        keys: b"RUN\r",
        check: Check::AnimatedDrawing,
    },
];

pub(super) struct Scenario {
    name: &'static str,
    pub(super) title: &'static str,
    files: &'static [&'static str],
    pub(super) steps: &'static [Step],
}

const F35_STEPS: [Step; 4] = [
    LOAD_STEPS[0],
    Step {
        label: "Find F35 drawing program 200 (file 1 is binary data)",
        keys: b"FIND@5:200\r",
        check: Check::Idle,
    },
    Step {
        label: "Load F35 BMP DISPLAY from file 200",
        keys: b"OLD@5:\r",
        check: Check::Idle,
    },
    DRAWING_STEPS[3],
];

static SCENARIOS: [Scenario; 11] = [
    Scenario {
        name: "simple_demo",
        title: "SimpleDemo",
        files: &FILES,
        steps: &STEPS,
    },
    Scenario {
        name: "battlestar",
        title: "Battlestar",
        files: &[],
        steps: &INPUT_STEPS,
    },
    Scenario {
        name: "star_wars",
        title: "StarWars",
        files: &[],
        steps: &DRAWING_STEPS,
    },
    Scenario {
        name: "benchmark",
        title: "Benchmark",
        files: &[],
        steps: &INPUT_STEPS,
    },
    Scenario {
        name: "asteroids",
        title: "Asteroids",
        files: &[],
        steps: &ANIMATION_STEPS,
    },
    Scenario {
        name: "f35",
        title: "F35",
        files: &[],
        steps: &F35_STEPS,
    },
    Scenario {
        name: "fractals",
        title: "Fractals",
        files: &[],
        steps: &INPUT_STEPS,
    },
    Scenario {
        name: "pictures",
        title: "Pictures",
        files: &[],
        steps: &INPUT_STEPS,
    },
    Scenario {
        name: "svg2fg",
        title: "SVG2FG",
        files: &[],
        steps: &INPUT_STEPS,
    },
    Scenario {
        name: "spacelab",
        title: "Spacelab",
        files: &[],
        steps: &INPUT_STEPS,
    },
    Scenario {
        name: "michlanglo",
        title: "Michlanglo",
        files: &[],
        steps: &INPUT_STEPS,
    },
];

pub(super) type Outcome = Result<(), String>;

pub(super) struct Session {
    pub(super) machine: Machine,
    pub(super) scenario: &'static Scenario,
    pub(super) imported_files: usize,
    runtime: HostRuntime,
    pub(super) step: usize,
    keys: Option<KeySequence>,
    step_elapsed: Duration,
    ready_elapsed: Duration,
    mouse: Option<u64>,
    before_run: Option<u64>,
    drawing_masks: Vec<u64>,
    pub(super) typed: String,
    pub(super) outcome: Option<Outcome>,
}

impl Session {
    pub(super) fn new(name: &str) -> Result<Self, String> {
        let scenario = SCENARIOS
            .iter()
            .find(|scenario| scenario.name == name)
            .ok_or_else(|| format!("unknown Flash Drive scenario: {name}"))?;
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../web/FlashDrive")
            .join(scenario.title);
        let names = if scenario.files.is_empty() {
            // Stable filename order makes duplicate-number replacement reproducible.
            // Only Finder metadata is excluded; invalid asset files remain import errors.
            let mut names = Vec::new();
            for entry in fs::read_dir(&directory)
                .map_err(|error| format!("cannot list {}: {error}", directory.display()))?
            {
                let entry = entry.map_err(|error| {
                    format!(
                        "cannot read directory entry in {}: {error}",
                        directory.display()
                    )
                })?;
                if entry.file_name() == ".DS_Store" {
                    continue;
                }
                if !entry
                    .file_type()
                    .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?
                    .is_file()
                {
                    return Err(format!(
                        "asset is not a regular file: {}",
                        entry.path().display()
                    ));
                }
                names.push(entry.file_name().into_string().map_err(|name| {
                    format!("asset filename is not UTF-8: {}", name.to_string_lossy())
                })?);
            }
            names.sort();
            if names.is_empty() {
                return Err(format!("no Flash Drive assets in {}", directory.display()));
            }
            names
        } else {
            scenario
                .files
                .iter()
                .map(|name| (*name).to_owned())
                .collect()
        };
        let files = names
            .iter()
            .map(|name| {
                let path = directory.join(name);
                fs::read(&path)
                    .map(|bytes| FlashDriveFile {
                        name: name.clone(),
                        bytes,
                    })
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut machine = Machine::default();
        machine
            .reset()
            .map_err(|error| format!("reset failed: {error}"))?;
        for (file, result) in files.iter().zip(machine.import_flash_files(&files)) {
            result.map_err(|error| format!("cannot import {}: {error}", file.name))?;
        }
        let mut runtime = HostRuntime::start(&mut machine);
        runtime
            .set_speed_factor(100.0)
            .map_err(|error| error.to_string())?;
        println!(
            "Imported {} unchanged {} files.",
            files.len(),
            scenario.title
        );
        println!("STEP 1: {}", scenario.steps[0].label);
        Ok(Self {
            machine,
            scenario,
            imported_files: files.len(),
            runtime,
            step: 0,
            keys: None,
            step_elapsed: Duration::ZERO,
            ready_elapsed: Duration::ZERO,
            mouse: None,
            before_run: None,
            drawing_masks: Vec::new(),
            typed: String::new(),
            outcome: None,
        })
    }

    pub(super) fn display(&mut self) -> &CpuReferenceRenderer {
        self.runtime.display(&mut self.machine)
    }

    // A fixed logical tick keeps keyboard timing independent of the caller's frame cadence.
    // Execution waits run at x100; actual command/answer entry runs at x1.
    pub(super) fn tick(&mut self) {
        if self.outcome.is_some() {
            return;
        }
        if let Some(sequence) = &mut self.keys {
            if let Some(KeyEvent::Press(code)) = sequence.press_event() {
                self.machine.keyboard_press(code);
                if code == b'\r' {
                    self.typed.push_str("<RETURN>");
                } else {
                    self.typed.push(char::from(code));
                }
            }
        }
        if let Err(error) = self.runtime.advance(&mut self.machine, TICK) {
            self.fail(format!("{}: {error}", self.scenario.steps[self.step].label));
            return;
        }
        self.step_elapsed += TICK;
        if self.step_elapsed >= STAGE_TIMEOUT {
            let indicators = self.machine.indicators();
            let stored_pixels = picture(self).0;
            self.fail(format!(
                "{} timed out: PC={:04X}, BUSY={}, I/O={}, stored pixels below row 200={}",
                self.scenario.steps[self.step].label,
                self.machine.registers().pc,
                indicators.busy,
                indicators.io,
                stored_pixels,
            ));
            return;
        }
        if let Some(sequence) = &mut self.keys {
            match sequence.advance(TICK) {
                Some(KeyEvent::Release) => self.machine.keyboard_release(),
                Some(KeyEvent::Finished) => {
                    self.keys = None;
                    if let Err(error) = self.runtime.set_speed_factor(100.0) {
                        self.fail(error.to_string());
                    }
                }
                Some(KeyEvent::Press(_)) => {
                    unreachable!("interval advancement does not press keys")
                }
                None => {}
            }
            return;
        }

        if matches!(self.scenario.steps[self.step].check, Check::AnimatedDrawing) {
            let frame = picture(self);
            let indicators = self.machine.indicators();
            let erasing = self.machine.display_erase_pending();
            self.observe_drawing(frame, indicators, erasing);
        }

        // Require stable readiness rather than a fixed sleep or the previous input prompt.
        if self.ready() {
            self.ready_elapsed += TICK;
        } else {
            self.ready_elapsed = Duration::ZERO;
        }
        if self.ready_elapsed >= Duration::from_millis(250) {
            if matches!(self.scenario.steps[self.step].check, Check::Mickey) {
                let fingerprint = picture(self).1;
                self.mouse = Some(fingerprint);
            }
            println!(
                "PASS step {}: {} [{}]",
                self.step + 1,
                self.scenario.steps[self.step].label,
                self.typed
            );
            self.step += 1;
            if self.step == self.scenario.steps.len() {
                self.outcome = Some(Ok(()));
                println!("PASS: {} scripted sequence completed.", self.scenario.title);
                return;
            }
            self.step_elapsed = Duration::ZERO;
            self.ready_elapsed = Duration::ZERO;
            self.typed.clear();
            let step = self.scenario.steps[self.step];
            let before_run = match step.check {
                Check::InputScreen => Some(stored_frame(self, 0).1),
                Check::FinishedDrawing | Check::AnimatedDrawing => Some(picture(self).1),
                _ => None,
            };
            self.before_run = before_run;
            self.keys = Some(KeySequence::new(step.keys));
            if let Err(error) = self.runtime.set_speed_factor(1.0) {
                self.fail(error.to_string());
            }
            println!("STEP {}: {}", self.step + 1, step.label);
        }
    }

    fn ready(&mut self) -> bool {
        let indicators = self.machine.indicators();
        match self.scenario.steps[self.step].check {
            Check::Boot => self.step_elapsed >= Duration::from_secs(1) && !indicators.busy,
            Check::Idle => !indicators.busy,
            Check::InputScreen => {
                let frame = stored_frame(self, 0);
                self.smoke_ready(
                    Check::InputScreen,
                    frame,
                    indicators,
                    self.machine.display_erase_pending(),
                )
            }
            check @ (Check::FinishedDrawing | Check::AnimatedDrawing) => {
                let frame = picture(self);
                self.smoke_ready(
                    check,
                    frame,
                    indicators,
                    self.machine.display_erase_pending(),
                )
            }
            _ if !indicators.io || self.machine.display_erase_pending() => false,
            check => {
                let (count, fingerprint) = picture(self);
                match check {
                    Check::Mickey | Check::Artwork => count > 500,
                    Check::DeathStar => {
                        count > 500 && self.mouse.is_some_and(|mouse| mouse != fingerprint)
                    }
                    Check::ArtPrompt => count <= 500,
                    Check::ReturnedMickey => count > 500 && self.mouse == Some(fingerprint),
                    Check::Boot
                    | Check::Idle
                    | Check::InputScreen
                    | Check::FinishedDrawing
                    | Check::AnimatedDrawing => unreachable!(),
                }
            }
        }
    }

    // Smoke observations share the same changed/stored-frame guard. INPUT may
    // keep BUSY lit; I/O, rather than command-idle state, signals its prompt.
    fn smoke_ready(
        &self,
        check: Check,
        (count, fingerprint): (usize, u64),
        indicators: MachineIndicators,
        erasing: bool,
    ) -> bool {
        if erasing || count <= 500 || !self.before_run.is_some_and(|before| before != fingerprint) {
            return false;
        }
        match check {
            Check::InputScreen => indicators.io,
            Check::FinishedDrawing => {
                !indicators.busy && !indicators.io && self.step_elapsed >= Duration::from_secs(1)
            }
            Check::AnimatedDrawing => {
                indicators.busy
                    && !indicators.io
                    && self.step_elapsed >= Duration::from_secs(1)
                    && self.drawing_masks.len() == 3
            }
            _ => unreachable!("only smoke stages use smoke observations"),
        }
    }

    fn observe_drawing(
        &mut self,
        (count, fingerprint): (usize, u64),
        indicators: MachineIndicators,
        erasing: bool,
    ) {
        if !indicators.busy
            || indicators.io
            || erasing
            || count <= 500
            || !self.before_run.is_some_and(|before| before != fingerprint)
        {
            self.drawing_masks.clear();
        } else if self.drawing_masks.len() < 3 && !self.drawing_masks.contains(&fingerprint) {
            self.drawing_masks.push(fingerprint);
        }
    }

    pub(super) fn fail_wall_timeout(&mut self) {
        if self.outcome.is_some() {
            return;
        }
        let indicators = self.machine.indicators();
        self.fail(format!(
            "overall {}-second deadline at step {} ({}), {:?} emulated stage time, PC={:04X}, BUSY={}, I/O={}",
            TOTAL_TIMEOUT.as_secs(),
            self.step + 1,
            self.scenario.steps[self.step].label,
            self.step_elapsed,
            self.machine.registers().pc,
            indicators.busy,
            indicators.io,
        ));
    }

    fn fail(&mut self, error: String) {
        self.machine.keyboard_release();
        eprintln!("FAIL: {error}");
        self.outcome = Some(Err(error));
    }
}

// Ignore transient cursor intensity and the text prompt, so picture comparisons
// observe stored graphics rather than changing keyboard echo or refresh pixels.
fn picture(session: &mut Session) -> (usize, u64) {
    stored_frame(session, 200)
}

fn stored_frame(session: &mut Session, first_row: usize) -> (usize, u64) {
    session.display().pixels()[first_row * CpuReferenceRenderer::WIDTH..]
        .iter()
        .fold((0, 0xcbf2_9ce4_8422_2325_u64), |(count, hash), &pixel| {
            let stored = pixel >= 240;
            (
                count + usize::from(stored),
                (hash ^ u64::from(stored)).wrapping_mul(0x100_0000_01b3),
            )
        })
}
