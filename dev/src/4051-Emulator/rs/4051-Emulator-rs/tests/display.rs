use std::time::Duration;

use tek4051_emulator::{
    cpu,
    display::{DisplayGranularity, DisplayModel},
    host_runtime::HostRuntime,
    machine::Machine,
};

const DISPLAY_Y_PORT_A: u16 = 0x878c;
const DISPLAY_Y_CONTROL_A: u16 = 0x878d;
const DISPLAY_Y_PORT_B: u16 = 0x878e;
const DISPLAY_Y_CONTROL_B: u16 = 0x878f;
const DISPLAY_X_PORT_A: u16 = 0x8794;
const DISPLAY_X_CONTROL_A: u16 = 0x8795;
const DISPLAY_X_PORT_B: u16 = 0x8796;
const DISPLAY_X_CONTROL_B: u16 = 0x8797;

fn configure_output_port(machine: &mut Machine, port: u16, control: u16, value: u8) {
    machine.write_byte(port, u8::MAX).unwrap();
    machine.write_byte(control, 0x04).unwrap();
    machine.write_byte(port, value).unwrap();
}

fn configure_display(
    machine: &mut Machine,
    y_output_a: u8,
    y_output_b: u8,
    x_output_a: u8,
    x_output_b: u8,
) {
    configure_output_port(machine, DISPLAY_Y_PORT_A, DISPLAY_Y_CONTROL_A, y_output_a);
    configure_output_port(machine, DISPLAY_Y_PORT_B, DISPLAY_Y_CONTROL_B, y_output_b);
    configure_output_port(machine, DISPLAY_X_PORT_A, DISPLAY_X_CONTROL_A, x_output_a);
    configure_output_port(machine, DISPLAY_X_PORT_B, DISPLAY_X_CONTROL_B, x_output_b);
}

fn start_idle_runtime(machine: &mut Machine) -> HostRuntime {
    machine.write_byte(0x0100, 0x20).unwrap();
    machine.write_byte(0x0101, 0xfe).unwrap();
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    HostRuntime::start(machine)
}

// What: Confirms PIA vector writes map through the reference offsets and phosphor rules.
// Why: The renderer needs a stable, machine-owned image rather than UI-specific draw calls.
#[test]
fn vector_segments_persist_while_refresh_cursors_decay() {
    // Arrange: Start an idle machine runtime before generating display bus activity.
    let mut machine = Machine::default();
    let mut runtime = start_idle_runtime(&mut machine);

    // Act: Configure the PIAs and clock a second endpoint to draw a horizontal vector.
    configure_display(&mut machine, 0, 20, 0x80, 5);
    machine.write_byte(DISPLAY_X_PORT_B, 10).unwrap();

    // Assert: The vector endpoint includes the source canvas offsets and y-axis inversion.
    let vector_row = DisplayModel::HEIGHT - (30 + 1);
    assert_eq!(machine.display().pixel_intensity(21, vector_row), Some(240));
    assert_eq!(machine.display().pixel_intensity(26, vector_row), Some(240));

    // Act: Draw a refresh vector, then read the Y port to request a cursor dot.
    machine.write_byte(DISPLAY_X_PORT_A, 0).unwrap();
    machine.write_byte(DISPLAY_X_PORT_B, 12).unwrap();
    assert_eq!(machine.display().pixel_intensity(27, 798), Some(200));
    machine.write_byte(DISPLAY_X_PORT_A, 0x40).unwrap();
    machine.write_byte(DISPLAY_Y_PORT_A, 0x04).unwrap();
    machine.read_byte(DISPLAY_Y_PORT_A).unwrap();

    // Assert: The un-stored cursor appears separately from the stored vector.
    assert_eq!(machine.display().pixel_intensity(28, 797), Some(200));
    assert_eq!(machine.display().pixel_intensity(29, 796), Some(200));

    // Act: Reach the 100 ms phosphor refresh callback.
    let revision_before_refresh = machine.display().revision();
    let events = runtime
        .advance(&mut machine, Duration::from_millis(100))
        .unwrap();

    // Assert: Refresh pixels decay while the stored vector remains in the framebuffer.
    assert_eq!(events.display_refreshes, 1);
    assert!(machine.display().revision() > revision_before_refresh);
    assert_eq!(machine.display().pixel_intensity(28, 797), Some(0));
    assert_eq!(machine.display().pixel_intensity(27, 798), Some(0));
    assert_eq!(machine.display().pixel_intensity(26, vector_row), Some(240));
}

// What: Confirms PIA reads plot character dots with fine, coarse, and mixed sizing.
// Why: Dot size and phosphor thresholds are visible compatibility behavior in the reference.
#[test]
fn character_dots_follow_each_display_granularity() {
    // Arrange: Pair each mode with its stored/cursor intensities and alpha-dot size.
    for (granularity, stored_intensity, cursor_intensity, alpha_dot_size) in [
        (DisplayGranularity::Fine, 254, 239, 1),
        (DisplayGranularity::Coarse, 240, 200, 2),
        (DisplayGranularity::Mixed, 254, 239, 2),
    ] {
        let mut machine = Machine::default();
        machine.set_display_granularity(granularity);
        configure_display(&mut machine, 0, 0, 0x40, 0);
        let initial_revision = machine.display().revision();

        // Act: Read Y to plot the cursor, then X and Y to mark and plot an alpha dot.
        machine.read_byte(DISPLAY_Y_PORT_A).unwrap();
        assert!(machine.display().revision() > initial_revision);
        assert_eq!(
            machine.display().pixel_intensity(16, 819),
            Some(cursor_intensity)
        );
        machine.read_byte(DISPLAY_X_PORT_A).unwrap();
        machine.write_byte(DISPLAY_X_PORT_A, 0x41).unwrap();
        machine.read_byte(DISPLAY_Y_PORT_A).unwrap();

        // Assert: The alpha dot overwrites its cursor pixel and uses the selected size.
        assert_eq!(
            machine.display().pixel_intensity(16, 819),
            Some(stored_intensity)
        );
        assert_eq!(
            machine.display().pixel_intensity(17, 819),
            Some(if alpha_dot_size == 2 {
                stored_intensity
            } else {
                0
            })
        );
        assert_eq!(
            machine.display().pixel_intensity(17, 818),
            Some(cursor_intensity)
        );
        assert_eq!(machine.display().granularity(), granularity);
        assert!(machine.display().revision() > initial_revision);
    }

    // Assert: Pixel inspection accepts the last row and rejects either out-of-range axis.
    let machine = Machine::default();
    assert_eq!(
        machine.display().pixel_intensity(DisplayModel::WIDTH, 0),
        None
    );
    assert_eq!(
        machine.display().pixel_intensity(0, DisplayModel::HEIGHT),
        None
    );
    assert_eq!(
        machine
            .display()
            .pixel_intensity(DisplayModel::WIDTH - 1, DisplayModel::HEIGHT - 1),
        Some(0)
    );
}
