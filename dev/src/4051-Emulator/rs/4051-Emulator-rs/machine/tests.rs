use super::{BasicKeyboardLatch, Machine, cpu};

// What: Track the base-ROM keyboard latch through a complete press and release.
// Why: Distinguish the firmware's scan latch from BASIC's later character consumption.
#[test]
fn base_rom_keyboard_latch_reports_observed_press_and_release() {
    // Arrange: Start the ROM at its display wait, then enable keyboard IRQ service.
    let mut machine = Machine::default();
    assert_eq!(
        machine.basic_keyboard_latch(),
        BasicKeyboardLatch::Unavailable
    );
    machine.reset().unwrap();
    machine.run_steps(100_000).unwrap();
    assert_eq!(machine.registers().pc, 0xc6b4);
    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    assert_eq!(machine.basic_keyboard_latch(), BasicKeyboardLatch::Released);

    // Act: Hold A and run until the ROM has written its key-down and key-code state.
    machine.keyboard_press(b'A');
    machine.keyboard_interrupt_tick();
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);
    for _ in 0..4_096 {
        if machine.basic_keyboard_latch() == BasicKeyboardLatch::Pressed(b'A') {
            break;
        }
        machine.step().unwrap();
    }
    // Assert: The ROM latch records the exact key code after its interrupt handler runs.
    assert_eq!(
        machine.basic_keyboard_latch(),
        BasicKeyboardLatch::Pressed(b'A')
    );

    // Act: Release A and run until the ROM clears its key-down state.
    machine.keyboard_release();
    machine.keyboard_interrupt_tick();
    for _ in 0..4_096 {
        if machine.basic_keyboard_latch() == BasicKeyboardLatch::Released {
            break;
        }
        machine.step().unwrap();
    }

    // Assert: The ROM clears its key-down flag after the release interrupt.
    assert_eq!(machine.basic_keyboard_latch(), BasicKeyboardLatch::Released);
}
