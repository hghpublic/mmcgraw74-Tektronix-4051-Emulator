use std::time::Duration;

use tek4051_emulator::{
    cpu,
    display::DisplayModel,
    host_runtime::{HostRuntime, HostRuntimeError, RuntimeEvents, SpeedFactorError},
    machine::{Machine, MachineBusError, PiaId, PiaPort},
};

fn type_tek_keys(machine: &mut Machine, runtime: &mut HostRuntime, keys: &[u8]) {
    for &key in keys {
        machine.keyboard_press(key);
        runtime
            .advance(machine, Duration::from_millis(100))
            .unwrap();
        machine.keyboard_release();
        runtime
            .advance(machine, Duration::from_millis(100))
            .unwrap();
    }
}

// What: Enter and run a numbered BASIC program through the machine keyboard.
// Why: This covers the boot, periodic interrupts, command entry, and rendered program output.
#[test]
fn keyboard_entered_basic_program_matches_reference_display() {
    // Arrange: Start the same reset and host-timing path used by the native application.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Enter "10 PRINT 1" followed by Return; 0x10 is the Tek Space key.
    type_tek_keys(&mut machine, &mut runtime, b"10\x10PRINT\x101\r");

    // Assert: The output line is empty before the program runs.
    let output_line = 104 * DisplayModel::WIDTH..118 * DisplayModel::WIDTH;
    assert!(
        machine.display().pixels()[output_line.clone()]
            .iter()
            .all(|&pixel| pixel < 240)
    );

    // Act: Run the program and allow the display refresh to settle.
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Assert: The printed "1" and the full stored-pixel image match the web reference.
    let pixels = machine.display().pixels();
    assert_eq!(
        pixels[output_line]
            .iter()
            .filter(|&&pixel| pixel >= 240)
            .count(),
        40
    );
    let stored_pixel_hash = pixels.iter().fold(0xcbf29ce484222325_u64, |hash, &pixel| {
        (hash ^ u64::from(pixel >= 240)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(stored_pixel_hash, 0x84a1e19a74efd07d);
}

// What: Confirms execution, keyboard refresh, and display updates follow the reference cadence.
// Why: Host frames must not run machine callbacks before their elapsed-time deadlines.
#[test]
fn host_intervals_drive_machine_and_display_callbacks() {
    // Arrange: Start from the ROM's display wait; runtime start releases it.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.run_steps(100_000).unwrap();
    assert_eq!(machine.registers().pc, 0xc6b4);
    let mut runtime = HostRuntime::start(&mut machine);

    // Act: Advance to just before the first 10 ms CPU and keyboard callbacks.
    let before_deadline = runtime
        .advance(&mut machine, Duration::from_millis(9))
        .unwrap();

    // Assert: No CPU, keyboard, or display callback has fired early.
    assert_eq!(machine.registers().pc, 0xc6b4);
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
    assert_eq!(before_deadline.display_refreshes, 0);

    // Act: Reach the 10 ms deadline, then advance to the 100 ms display boundary.
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();
    assert_ne!(machine.registers().pc, 0xc6b4);
    assert_ne!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);

    // Act: Advance to 99 ms, then reach the display refresh deadline.
    let before_display_deadline = runtime
        .advance(&mut machine, Duration::from_millis(89))
        .unwrap();
    let at_display_deadline = runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: The display callback is due only at 100 ms.
    assert_eq!(before_display_deadline.display_refreshes, 0);
    assert_eq!(at_display_deadline.display_refreshes, 1);

    // Assert: Keyboard refresh remains latched after the display boundary.
    assert_ne!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
}

// What: Pause execution across elapsed wall time and then resume at a fresh callback boundary.
// Why: Paused time must not run CPU or display callbacks or shorten the next interval.
#[test]
fn pause_freezes_callbacks_and_resume_restarts_the_interval() {
    // Arrange: Use RAM NOPs so the CPU position reveals any executed callback.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..2_265 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    machine.write_byte(0x87a9, 0x04).unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_millis(5))
        .unwrap();
    runtime.pause();

    // Act: Advance one paused second, resume, and test both sides of a full 10 ms interval.
    let paused_events = runtime
        .advance(&mut machine, Duration::from_secs(1))
        .unwrap();
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
    runtime.resume().unwrap();
    runtime
        .advance(&mut machine, Duration::from_millis(9))
        .unwrap();
    let before_deadline = machine.registers().pc;
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: Paused time ran no callbacks, and resumed execution waited 10 ms.
    assert_eq!(paused_events.display_refreshes, 0);
    assert_eq!(before_deadline, program_start);
    assert_eq!(machine.registers().pc, program_start + 2_264);
    assert_ne!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
}

// What: Confirms the speed factor scales the CPU instruction budget per 10 ms interval.
// Why: The reference exposes a speed multiplier independently of its callback cadence.
#[test]
fn speed_factor_scales_instructions_per_execution_interval() {
    // Arrange: Fill RAM with one-byte NOPs so the PC records the executed instruction count.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..4_529 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.set_speed_factor(2.0).unwrap();

    // Act: Advance through one 10 ms execution callback.
    runtime
        .advance(&mut machine, Duration::from_millis(10))
        .unwrap();

    // Assert: The source formula yields a 4,528-instruction budget at speed factor 2.
    assert_eq!(machine.registers().pc, program_start + 4_528);
}

// What: Confirms a display erase remains busy for 500 ms before host completion.
// Why: The CPU-visible display-ready signal must follow the reference's erase delay.
#[test]
fn display_erase_completes_after_five_hundred_milliseconds() {
    // Arrange: Run a masked branch loop and trigger the display's erase handshake.
    let mut machine = Machine::default();
    machine.write_byte(0x0100, 0x20).unwrap();
    machine.write_byte(0x0101, 0xfe).unwrap();
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    machine.set_pia_input(PiaId::DisplayX, PiaPort::A, 0xa0);
    let mut runtime = HostRuntime::start(&mut machine);

    machine.write_byte(0x8794, 0x10).unwrap();
    machine.write_byte(0x8794, 0).unwrap();
    machine.write_byte(0x8795, 0x04).unwrap();
    machine.write_byte(0x8794, 0x10).unwrap();
    machine.write_byte(0x8797, 0x04).unwrap();
    machine.write_byte(0x8796, 0x10).unwrap();
    machine.write_byte(0x8796, 0).unwrap();
    machine.write_byte(0x8794, 0).unwrap();
    assert!(machine.display_erase_pending());
    assert_eq!(machine.display().pixel_intensity(0, 0), Some(u8::MAX));
    assert_eq!(
        machine
            .display()
            .pixel_intensity(DisplayModel::WIDTH / 2, DisplayModel::HEIGHT / 2),
        Some(u8::MAX)
    );
    assert_eq!(
        machine
            .display()
            .pixel_intensity(DisplayModel::WIDTH - 1, DisplayModel::HEIGHT - 1),
        Some(u8::MAX)
    );

    // Act: Pause during erase, then resume and advance to the 499 ms boundary.
    runtime
        .advance(&mut machine, Duration::from_millis(200))
        .unwrap();
    runtime.pause();
    let paused_events = runtime
        .advance(&mut machine, Duration::from_secs(1))
        .unwrap();
    assert!(machine.display_erase_pending());
    assert_eq!(paused_events.display_refreshes, 0);
    runtime.resume().unwrap();
    let before_completion = runtime
        .advance(&mut machine, Duration::from_millis(499))
        .unwrap();
    assert!(machine.display_erase_pending());
    assert_eq!(machine.display().pixel_intensity(0, 0), Some(u8::MAX));
    assert_eq!(
        machine
            .display()
            .pixel_intensity(DisplayModel::WIDTH / 2, DisplayModel::HEIGHT / 2),
        Some(u8::MAX)
    );
    let revision_before_completion = machine.display().revision();
    let at_completion = runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: The erase stays pending until the 500 ms boundary and is then acknowledged.
    assert!(!machine.display_erase_pending());
    assert!(machine.display().revision() > revision_before_completion);
    assert_eq!(machine.display().pixel_intensity(0, 0), Some(0));
    assert_eq!(
        machine
            .display()
            .pixel_intensity(DisplayModel::WIDTH / 2, DisplayModel::HEIGHT / 2),
        Some(0)
    );
    assert_eq!(
        machine
            .display()
            .pixel_intensity(DisplayModel::WIDTH - 1, DisplayModel::HEIGHT - 1),
        Some(0)
    );
    assert_eq!(before_completion.display_refreshes, 4);
    assert_eq!(at_completion.display_refreshes, 1);
}

// What: Confirms rejected speed factors do not replace a previously selected instruction budget.
// Why: A failed setting change must preserve the last valid emulation speed.
#[test]
fn invalid_speed_factor_preserves_the_active_instruction_budget() {
    // Arrange: Select x2 speed and place enough one-byte NOPs in RAM to observe its budget.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..2_265 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.set_speed_factor(2.0).unwrap();

    // Act: Reject zero, non-finite, and overflowing speed factors.
    assert_eq!(
        runtime.set_speed_factor(0.0),
        Err(SpeedFactorError::NotFiniteAndPositive)
    );
    assert_eq!(
        runtime.set_speed_factor(f64::NAN),
        Err(SpeedFactorError::NotFiniteAndPositive)
    );
    assert_eq!(
        runtime.set_speed_factor(f64::MAX),
        Err(SpeedFactorError::InstructionBudgetOverflow)
    );

    // Act: Execute one 10 ms interval after the rejected changes.
    runtime
        .advance(&mut machine, Duration::from_millis(10))
        .unwrap();

    // Assert: The valid x2 budget remains 4,528 one-byte instructions.
    assert_eq!(machine.registers().pc, program_start + 4_528);
}

// What: Confirms a machine error stops timing, including at a shared 100 ms boundary.
// Why: Failed CPU callbacks must not leave elapsed counters due and cause a later zero-step loop.
#[test]
fn machine_error_stops_runtime_at_a_callback_boundary() {
    // Arrange: Advance to 90 ms in RAM, then select an unsupported bank at the 100 ms boundary.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..20_377 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_millis(90))
        .unwrap();
    machine.write_byte(0x87c0, 0x08).unwrap();
    machine.cpu_mut().registers.pc = 0x9800;

    // Act: The next CPU callback fails on the same boundary as display refresh.
    let result = runtime.advance(&mut machine, Duration::from_millis(10));

    // Assert: The bus error is returned and this instance refuses to resume.
    assert_eq!(
        result,
        Err(HostRuntimeError::Machine {
            source: cpu::CpuError::Bus(MachineBusError::UnsupportedRomBank {
                selector: 1,
                address: 0x9800,
            }),
            events: RuntimeEvents {
                display_refreshes: 1,
            },
        })
    );
    assert_eq!(
        runtime.advance(&mut machine, Duration::from_millis(1)),
        Err(HostRuntimeError::Stopped)
    );
    assert_eq!(runtime.resume(), Err(HostRuntimeError::Stopped));
}
