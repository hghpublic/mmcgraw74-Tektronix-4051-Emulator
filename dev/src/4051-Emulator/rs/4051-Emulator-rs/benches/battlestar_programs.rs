//! Measures Flash Drive import, ROM launch-command delivery, and a fixed
//! simulated-time window after command delivery for Battlestar programs 2, 8, and 13.
//!
//! Each sample boots a fresh machine, imports the original Flash Drive file,
//! and launches it through the same FIND/OLD/RUN keyboard sequence as the GUI.
//! The timed window starts after those keys are delivered; the public session
//! API does not expose a signal that BASIC has entered the selected program.
//! Five minutes is longer than file 13's documented four-minute initialization,
//! but does not confirm that initialization completed in the timed window.

use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use tek4051_emulator::{
    emulation::{Command, EmulationState, TICK},
    emulation_input::AutoLoadState,
    machine::{Machine, gpib::FlashDriveFile},
};

const FILE_NUMBERS: [u8; 3] = [2, 8, 13];
const WARM_UP_SAMPLES: usize = 1;
const MEASURED_SAMPLES: usize = 3;
const BOOT_TIME: Duration = Duration::from_secs(20);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(60);
const POST_COMMAND_WINDOW: Duration = Duration::from_secs(300);
const LAUNCH_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct Sample {
    import_time: Duration,
    launch_time: Duration,
    post_command_window_time: Duration,
    final_pc: u16,
}

fn battlestar_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/FlashDrive/Battlestar")
}

fn read_program(number: u8) -> Result<FlashDriveFile, Box<dyn Error>> {
    let directory = battlestar_directory();
    let number_field = number.to_string();
    let mut matching_path = None;
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let mut fields = name.split_whitespace();
        if fields.next() == Some(number_field.as_str())
            && fields.next() == Some("ASCII")
            && fields.next() == Some("PROG")
            && matching_path.replace((path, name)).is_some()
        {
            return Err(io::Error::other(format!(
                "multiple ASCII PROG files found for Battlestar file {number}"
            ))
            .into());
        }
    }

    let Some((path, name)) = matching_path else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Battlestar ASCII PROG file {number} was not found in {}",
                directory.display()
            ),
        )
        .into());
    };

    Ok(FlashDriveFile {
        name,
        bytes: fs::read(path)?,
    })
}

fn advance_for(state: &mut EmulationState, now: &mut Instant, duration: Duration) {
    let mut remaining = duration;
    while !remaining.is_zero() {
        let elapsed = remaining.min(TICK);
        *now += elapsed;
        state.advance_to(*now);
        remaining -= elapsed;
    }
}

fn check_runtime(state: &mut EmulationState, phase: &str) -> Result<(), Box<dyn Error>> {
    let snapshot = state.snapshot();
    if let Some(error) = &snapshot.runtime_error {
        return Err(io::Error::other(format!("{phase}: {error}")).into());
    }
    if let Some(error) = &snapshot.auto_load_error {
        return Err(io::Error::other(format!("{phase}: {error}")).into());
    }
    Ok(())
}

fn run_sample(number: u8) -> Result<Sample, Box<dyn Error>> {
    let program = read_program(number)?;
    let mut machine = Machine::default();

    let import_started = Instant::now();
    let results = machine.import_flash_files(std::slice::from_ref(&program));
    let import_time = import_started.elapsed();
    match results.as_slice() {
        [Ok(())] => {}
        [Err(error)] => {
            return Err(io::Error::other(format!(
                "Flash Drive import failed for file {number}: {error:?}"
            ))
            .into());
        }
        _ => {
            return Err(io::Error::other(format!(
                "Flash Drive import returned an unexpected result for file {number}"
            ))
            .into());
        }
    }

    machine
        .reset()
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut state = EmulationState::new(machine);
    let mut now = Instant::now();
    state.apply(Command::Run(true)).map_err(io::Error::other)?;
    advance_for(&mut state, &mut now, BOOT_TIME);
    check_runtime(&mut state, "ROM boot")?;
    state
        .apply(Command::PromptReady {
            ready: true,
            request: 1,
        })
        .map_err(io::Error::other)?;
    state
        .apply(Command::Launch(number))
        .map_err(io::Error::other)?;
    let launch = state.snapshot();
    if let Some((true, message)) = &launch.program_launch_notice {
        return Err(io::Error::other(format!(
            "could not launch Flash Drive file {number}: {message}"
        ))
        .into());
    }

    let launch_started = Instant::now();
    let mut launch_virtual_time = Duration::ZERO;
    let launch_time = loop {
        if launch_virtual_time >= LAUNCH_TIMEOUT {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("launch commands for file {number} were not delivered"),
            )
            .into());
        }
        advance_for(&mut state, &mut now, LAUNCH_POLL_INTERVAL);
        launch_virtual_time += LAUNCH_POLL_INTERVAL;
        check_runtime(&mut state, "launch command delivery")?;
        if state.snapshot().auto_load_state == AutoLoadState::Sent {
            break launch_started.elapsed();
        }
    };

    let window_started = Instant::now();
    advance_for(&mut state, &mut now, POST_COMMAND_WINDOW);
    check_runtime(&mut state, "post-command window")?;
    let post_command_window_time = window_started.elapsed();
    let final_pc = state.snapshot().registers.pc;

    Ok(Sample {
        import_time,
        launch_time,
        post_command_window_time,
        final_pc,
    })
}

fn average_ms(samples: &[Sample], duration: fn(&Sample) -> Duration) -> f64 {
    samples
        .iter()
        .map(|sample| duration(sample).as_secs_f64() * 1_000.0)
        .sum::<f64>()
        / samples.len() as f64
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "Battlestar Flash Drive benchmark: {WARM_UP_SAMPLES} warm-up, \
         {MEASURED_SAMPLES} measured samples, {POST_COMMAND_WINDOW:?} simulated after command delivery"
    );
    for number in FILE_NUMBERS {
        let mut samples = Vec::with_capacity(MEASURED_SAMPLES);
        for iteration in 0..WARM_UP_SAMPLES + MEASURED_SAMPLES {
            let sample = run_sample(number)?;
            std::hint::black_box(sample.final_pc);
            if iteration >= WARM_UP_SAMPLES {
                samples.push(sample);
            }
        }

        let import_ms = average_ms(&samples, |sample| sample.import_time);
        let launch_ms = average_ms(&samples, |sample| sample.launch_time);
        let window_ms = average_ms(&samples, |sample| sample.post_command_window_time);
        let realtime_factor = POST_COMMAND_WINDOW.as_secs_f64() * 1_000.0 / window_ms;
        println!(
            "File {number}: machine import {import_ms:.3} ms; launch keys delivered in \
             {launch_ms:.3} ms; {POST_COMMAND_WINDOW:?} after delivery {window_ms:.3} ms \
             ({realtime_factor:.2}x simulated/wall; BASIC program start unverified)"
        );
    }
    Ok(())
}
