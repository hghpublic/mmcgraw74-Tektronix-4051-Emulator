use tek4051_emulator::{
    cpu,
    machine::{KeyboardModifier, Machine, MachineBusError, PiaControlLine, PiaId, PiaPort},
    rom::{
        BASE_ROM, BASE_ROM_FNV1A64, BASE_ROM_SIZE, LEFT_BACKPACK_ROM, LEFT_BACKPACK_ROM_FNV1A64,
        LEFT_BACKPACK_ROM_SIZE, LEFT_ROM_EXPANDER_BANK, LEFT_ROM_EXPANDER_BANK_FNV1A64,
        LEFT_ROM_EXPANDER_BANK_SIZE, LEFT_ROM_EXPANDER_SLOT_COUNT, OVERFLOW_ROM,
        OVERFLOW_ROM_FNV1A64, OVERFLOW_ROM_SIZE, RIGHT_BACKPACK_ROM, RIGHT_BACKPACK_ROM_FNV1A64,
        RIGHT_BACKPACK_ROM_SIZE, RIGHT_ROM_EXPANDER_BANK, RIGHT_ROM_EXPANDER_BANK_FNV1A64,
        RIGHT_ROM_EXPANDER_BANK_SIZE, RIGHT_ROM_EXPANDER_SLOT_COUNT, ROM_EXPANDER_SLOT_SIZE,
    },
};

const ROM_BASE: u16 = 0x8000;
const BANK_SELECT_REGISTER: u16 = 0x87c0;

// What: Confirms each imported ROM image retains its source length and fingerprint.
// Why: The machine maps these bytes directly, so accidental changes alter ROM-visible behavior.
#[test]
fn rom_images_have_the_expected_lengths_and_fingerprints() {
    // Arrange: Pair each image with its expected length and source-derived fingerprint.
    for (rom, expected_size, expected_fingerprint) in [
        (BASE_ROM.as_slice(), BASE_ROM_SIZE, BASE_ROM_FNV1A64),
        (
            OVERFLOW_ROM.as_slice(),
            OVERFLOW_ROM_SIZE,
            OVERFLOW_ROM_FNV1A64,
        ),
        (
            LEFT_BACKPACK_ROM.as_slice(),
            LEFT_BACKPACK_ROM_SIZE,
            LEFT_BACKPACK_ROM_FNV1A64,
        ),
        (
            RIGHT_BACKPACK_ROM.as_slice(),
            RIGHT_BACKPACK_ROM_SIZE,
            RIGHT_BACKPACK_ROM_FNV1A64,
        ),
        (
            LEFT_ROM_EXPANDER_BANK.as_slice(),
            LEFT_ROM_EXPANDER_BANK_SIZE,
            LEFT_ROM_EXPANDER_BANK_FNV1A64,
        ),
        (
            RIGHT_ROM_EXPANDER_BANK.as_slice(),
            RIGHT_ROM_EXPANDER_BANK_SIZE,
            RIGHT_ROM_EXPANDER_BANK_FNV1A64,
        ),
    ] {
        // Act: Compute a fingerprint over the image bytes.
        let fingerprint = rom.iter().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });

        // Assert: The image length and fingerprint match the imported source.
        assert_eq!(rom.len(), expected_size);
        assert_eq!(fingerprint, expected_fingerprint);
    }
}

// What: Reset a default machine using its embedded base-ROM vector.
// Why: Machine startup depends on the CPU entering the ROM with reset flags.
#[test]
fn reset_loads_the_base_rom_vector() {
    // Arrange: Start with a default machine and its mapped ROM.
    let mut machine = Machine::default();
    // Act: Reset the CPU through the machine.
    machine.reset().unwrap();

    // Assert: PC and CCR match the expected ROM entry and reset state.
    assert_eq!(machine.registers().pc, 0xbc4b);
    assert_eq!(machine.registers().ccr, 0xd0);
}

// What: Execute the first twelve instructions after base-ROM reset.
// Why: Startup initialization must establish the expected registers and RAM values.
#[test]
fn base_rom_executes_its_reset_initialization_sequence() {
    // Arrange: Enter the base-ROM reset vector.
    let mut machine = Machine::default();
    machine.reset().unwrap();

    // Act: Run the bounded initialization sequence.
    assert_eq!(machine.run_steps(12).unwrap(), 12);

    // Assert: Its observed register and RAM results match the expected checkpoint.
    assert_eq!(machine.registers().pc, 0xbc6b);
    assert_eq!(machine.registers().sp, 0x0028);
    assert_eq!(machine.registers().ix, 0x7ff5);
    assert_eq!(machine.read_byte(0x0008).unwrap(), 0);
    assert_eq!(machine.read_byte(0x7fff).unwrap(), 0);
}

// What: Run startup without a host display-ready signal and sample its wait loop.
// Why: Startup must expose the expected masked display-wait checkpoint.
#[test]
fn base_rom_waits_for_display_readiness_with_interrupts_masked() {
    // Arrange: Reset without marking the display ready.
    let mut machine = Machine::default();
    machine.reset().unwrap();

    // Act: Run to the bounded startup checkpoint.
    assert_eq!(machine.run_steps(100_000).unwrap(), 100_000);
    // Assert: The CPU is at the expected masked wait.
    assert_eq!(machine.registers().pc, 0xc6b4);
    assert_ne!(machine.registers().ccr & cpu::INTERRUPT_MASK, 0);

    let loop_registers = machine.registers();
    // Act: Execute another twelve instructions in the wait.
    assert_eq!(machine.run_steps(12).unwrap(), 12);
    // Assert: The complete register state returns to the sampled state.
    assert_eq!(machine.registers(), loop_registers);
}

// What: Exercise display erase writes before and after selecting the data register.
// Why: Only a falling edge on the relevant output should start the host-completed erase.
#[test]
fn display_erase_requires_a_data_output_falling_edge_and_host_completion() {
    // Arrange: Set display input bits and mark the display ready.
    let mut machine = Machine::default();
    machine.set_pia_input(PiaId::DisplayX, PiaPort::A, 0xa0);
    machine.display_ready();

    // Act: Toggle the direction register, then select and read the data register.
    machine.write_byte(0x8794, 0x10).unwrap();
    machine.write_byte(0x8794, 0).unwrap();
    assert!(!machine.display_erase_pending());
    machine.write_byte(0x8795, 0x04).unwrap();
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa4);

    // Act: Set the erase output high and toggle a different data port.
    machine.write_byte(0x8794, 0x10).unwrap();
    assert!(!machine.display_erase_pending());
    machine.write_byte(0x8797, 0x04).unwrap();
    machine.write_byte(0x8796, 0x10).unwrap();
    machine.write_byte(0x8796, 0).unwrap();
    assert!(!machine.display_erase_pending());

    // Act: Drive the erase output low.
    machine.write_byte(0x8794, 0).unwrap();
    // Assert: Erase is pending and readiness stays low across reads.
    assert!(machine.display_erase_pending());
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa0);
    assert!(machine.display_erase_pending());

    // Act: Complete the erase and repeat the low write.
    machine.display_ready();
    // Assert: Readiness returns and another low write does not retrigger erase.
    assert!(!machine.display_erase_pending());
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa4);
    machine.write_byte(0x8794, 0).unwrap();
    assert!(!machine.display_erase_pending());
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa4);
}

// What: Confirms startup probes the ROM packs and processes keyboard input through the base ROM.
// Why: This traces initialization and a key event without directly overriding CPU registers.
#[test]
fn startup_probes_rom_packs_and_processes_keyboard_input() {
    // Arrange: Run reset to its display wait with the interrupt mask still set.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.run_steps(100_000).unwrap();
    assert_eq!(machine.registers().pc, 0xc6b4);
    assert_eq!(machine.read_byte(0x8794).unwrap() & 0x0c, 0);
    assert_ne!(machine.registers().ccr & cpu::INTERRUPT_MASK, 0);

    // Act: Release the display wait and track startup through selector 7's first slot.
    machine.display_ready();
    let mut executed_overflow = false;
    let mut probed_ignored_banks = [false; 2];
    let mut probed_left_backpack_header = false;
    let mut probed_right_backpack_header = false;
    let mut probed_left_expander_slot_header = false;
    let mut probed_right_expander_slot_header = false;
    for _ in 0..200_000 {
        let (selector, bank_address) = machine.bank_selection();
        executed_overflow |= selector == 1 && (0x8800..0x9800).contains(&machine.registers().pc);
        let probing_ignored_bank = machine.registers().pc == 0xaa8f && (2..=3).contains(&selector);
        let probing_left_backpack_header = machine.registers().pc == 0xaa8f && selector == 4;
        let probing_right_backpack_header = machine.registers().pc == 0xaa8f && selector == 5;
        let probing_left_expander_slot_header =
            machine.registers().pc == 0xaa8f && selector == 6 && bank_address == 0;
        let probing_right_expander_slot_header =
            machine.registers().pc == 0xaa8f && selector == 7 && bank_address == 0;
        machine.step().unwrap();
        if probing_ignored_bank {
            assert_eq!(machine.registers().ix, 0);
            probed_ignored_banks[usize::from(selector - 2)] = true;
        }
        if probing_left_backpack_header {
            assert_eq!(machine.registers().ix, 0x4051);
            probed_left_backpack_header = true;
        }
        if probing_right_backpack_header {
            // DERROMR is all 0xFF, so its probed header word is 0xFFFF.
            assert_eq!(machine.registers().ix, 0xffff);
            probed_right_backpack_header = true;
        }
        if probing_left_expander_slot_header {
            assert_eq!(machine.registers().ix, 0x4051);
            probed_left_expander_slot_header = true;
        }
        if probing_right_expander_slot_header {
            assert_eq!(machine.registers().ix, 0x4051);
            probed_right_expander_slot_header = true;
            break;
        }
    }

    // Assert: Startup reaches the mapped right slot without changing CPU state directly.
    assert!(executed_overflow);
    assert_eq!(probed_ignored_banks, [true; 2]);
    assert!(probed_left_backpack_header);
    assert!(probed_right_backpack_header);
    assert!(probed_left_expander_slot_header);
    assert!(probed_right_expander_slot_header);
    assert_eq!(machine.bank_selection(), (7, 0));
    assert!(!machine.display_erase_pending());
    // Startup code has cleared the CPU interrupt mask by the right-expander probe.
    assert_eq!(machine.registers().ccr & cpu::INTERRUPT_MASK, 0);

    // Act: Continue startup until the observed keyboard routine or the instruction bound.
    let mut reached_keyboard_routine = false;
    for _ in 0..2_000_000 {
        if machine.registers().pc == 0xc88a {
            reached_keyboard_routine = true;
            break;
        }
        machine.step().unwrap();
    }
    reached_keyboard_routine |= machine.registers().pc == 0xc88a;

    // Assert: Normal startup reaches the keyboard routine without an error.
    assert!(reached_keyboard_routine);

    // Act: Let the base-ROM keyboard handler consume a press and release.
    machine.keyboard_press(b'A');
    machine.keyboard_interrupt_tick();
    let mut serviced_keyboard_irq = false;
    for _ in 0..4_096 {
        let outcome = machine.step().unwrap();
        serviced_keyboard_irq |= outcome.pc_before == 0xefb8;
        if machine.read_byte(0x0597).unwrap() == 1 && machine.read_byte(0x059c).unwrap() == b'A' {
            break;
        }
    }

    // Assert: The real IRQ handler records the pressed key.
    assert!(serviced_keyboard_irq);
    assert_eq!(machine.read_byte(0x0597).unwrap(), 1);
    assert_eq!(machine.read_byte(0x059c).unwrap(), b'A');

    // Act: Deliver the corresponding key release and keyboard interrupt tick.
    machine.keyboard_release();
    machine.keyboard_interrupt_tick();
    for _ in 0..4_096 {
        if machine.read_byte(0x0597).unwrap() == 0 {
            break;
        }
        machine.step().unwrap();
    }

    // Assert: The ROM clears the key-down state but retains the last key value.
    assert_eq!(machine.read_byte(0x0597).unwrap(), 0);
    assert_eq!(machine.read_byte(0x059c).unwrap(), b'A');
}

// What: Write to RAM, base ROM, and an unmapped I/O location.
// Why: The bus must distinguish writable memory from ROM and unmapped I/O.
#[test]
fn ram_is_writable_rom_is_read_only_and_unmapped_io_reads_zero() {
    // Arrange: Capture the original ROM byte in a default machine.
    let mut machine = Machine::default();
    let rom_byte = machine.read_byte(0x8000).unwrap();

    // Act: Write distinct values to the three address regions.
    machine.write_byte(0x7fff, 0xa5).unwrap();
    machine.write_byte(0x8000, !rom_byte).unwrap();
    machine.write_byte(0x8780, 0xff).unwrap();

    // Assert: Only RAM changes; ROM retains its byte and unmapped I/O reads zero.
    assert_eq!(machine.read_byte(0x7fff).unwrap(), 0xa5);
    assert_eq!(machine.read_byte(0x8000).unwrap(), rom_byte);
    assert_eq!(machine.read_byte(0x8780).unwrap(), 0);
}

// What: Select a PIA direction register, then combine output and input bits on read.
// Why: Firmware accesses the direction and data registers through the same address.
#[test]
fn pia_data_register_muxes_input_and_output_bits_by_direction() {
    // Arrange: Configure the lower nibble as output and verify the direction register.
    let mut machine = Machine::default();
    machine
        .write_byte(0x878c, 0x0f)
        .expect("PIA writes cannot fail");
    assert_eq!(machine.read_byte(0x878c).unwrap(), 0x0f);

    // Act: Select the data register, write output bits, and supply input bits.
    machine
        .write_byte(0x878d, 0xff)
        .expect("PIA writes cannot fail");
    machine
        .write_byte(0x878c, 0x0c)
        .expect("PIA writes cannot fail");
    machine.set_pia_input(PiaId::DisplayY, PiaPort::A, 0xa5);

    // Assert: Control is masked and the data read merges the two nibbles.
    assert_eq!(machine.read_byte(0x878d).unwrap(), 0x3f);
    assert_eq!(machine.read_byte(0x878c).unwrap(), 0xac);
}

// What: Observe keyboard key data, modifiers, release, and Caps Lock through PIA ports.
// Why: The CPU reads key codes and active-low modifier state from separate ports.
#[test]
fn keyboard_ports_expose_key_release_modifiers_and_caps_lock() {
    // Arrange: Select the keyboard data registers on both ports.
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();

    // Act: Press a key and toggle the modifiers independently and together.
    // Assert: Key data and active-low modifier bits follow each change.
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);
    machine.keyboard_press(b'A');
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xc1);

    machine.set_keyboard_modifier(KeyboardModifier::Shift, true);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0a);
    machine.set_keyboard_modifier(KeyboardModifier::Shift, false);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);

    machine.set_keyboard_modifier(KeyboardModifier::Control, true);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x09);
    machine.set_keyboard_modifier(KeyboardModifier::Control, false);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);

    machine.set_keyboard_modifier(KeyboardModifier::Shift, true);
    machine.set_keyboard_modifier(KeyboardModifier::Control, true);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x08);

    // Act: Release the key, toggle Caps Lock, and press a different key.
    machine.keyboard_release();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    machine.toggle_caps_lock();
    machine.keyboard_press(b'B');
    // Assert: The second key reflects the toggled Caps Lock bit.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x42);
}

// What: Signal keyboard PIA control edges and read the associated data register.
// Why: Selected edges latch status until a data read clears the flag.
#[test]
fn pia_control_edges_latch_status_and_data_reads_clear_it() {
    // Arrange: Select rising-edge CA1 detection with the data register enabled.
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x07).unwrap();

    // Act: Signal the rising edge, then read the port data.
    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, true);
    // Assert: The edge is latched and the data read clears it.
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x87);

    machine.read_byte(0x87a8).unwrap();
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x07);

    // Act: Signal the unselected edge and switch to falling-edge detection.
    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, false);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x07);

    machine.write_byte(0x87a9, 0x05).unwrap();
    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, true);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x05);
    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, false);
    // Assert: Only the newly selected falling edge sets the status flag.
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x85);
}

// What: Raise each PIA control-line flag and step the machine.
// Why: Board IRQ/NMI routing uses latched flags even when PIA enables are clear.
#[test]
fn board_interrupt_flags_reach_rom_vectors_without_pia_enable_gating() {
    // Arrange: Cover every routed PIA and each of its four control lines.
    for (pia, base, vector) in [
        (PiaId::DisplayX, 0x8794, 0xefb8),
        (PiaId::Keyboard, 0x87a8, 0xefb8),
        (PiaId::Gpib, 0x87b0, 0xefb8),
        (PiaId::DisplayY, 0x878c, 0xfdbc),
        (PiaId::Tape, 0x8798, 0xfdbc),
    ] {
        for (line, offset, control, flag) in [
            (PiaControlLine::Ca1, 1, 0x06, 0x80),
            (PiaControlLine::Ca2, 1, 0x0c, 0x40),
            (PiaControlLine::Cb1, 3, 0x06, 0x80),
            (PiaControlLine::Cb2, 3, 0x0c, 0x40),
        ] {
            // Arrange: Reset with the selected line configured and IRQ unmasked when needed.
            let mut machine = Machine::default();
            machine.reset().unwrap();
            machine.cpu_mut().registers.pc = 0x2000;
            if vector == 0xefb8 {
                machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
            }
            machine.write_byte(base + offset, control).unwrap();
            // Act: Raise the selected line and sample its status.
            machine.signal_pia_control_line(pia, line, true);

            // Assert: The flag is latched and the next step enters the board's ROM vector.
            assert_eq!(
                machine.read_byte(base + offset).unwrap(),
                control | flag,
                "{pia:?} {line:?} flag"
            );
            assert_eq!(
                machine.step().unwrap().pc_before,
                vector,
                "{pia:?} {line:?} vector"
            );
        }
    }
}

// What: Raise a keyboard IRQ while masked, then clear the CPU mask.
// Why: A pending IRQ must remain observable until the CPU can service it.
#[test]
fn cpu_mask_defers_keyboard_irq_until_unmasked() {
    // Arrange: Place a NOP in RAM and latch a keyboard interrupt after reset.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.cpu_mut().registers.pc = 0x2000;
    machine.write_byte(0x2000, 0x01).unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.keyboard_interrupt_tick();

    // Act: Step while masked.
    // Assert: The RAM instruction runs and the keyboard flag remains set.
    assert_eq!(machine.step().unwrap().pc_before, 0x2000);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x84);

    // Act: Unmask IRQ and step again.
    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    // Assert: The step begins at the ROM IRQ handler.
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);
}

// What: Hold NMI alongside IRQ, clear it, and raise it again.
// Why: NMI has priority and must not repeatedly interrupt while its flag remains set.
#[test]
fn held_nmi_takes_priority_over_irq_and_rearms_after_clearing() {
    // Arrange: Latch keyboard IRQ and DisplayY NMI on a reset machine.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    machine.write_byte(0x2000, 0x01).unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.keyboard_interrupt_tick();
    machine.write_byte(0x878d, 0x06).unwrap();
    machine.signal_pia_control_line(PiaId::DisplayY, PiaControlLine::Ca1, true);

    // Act: Step with both sources pending, then step again with NMI held.
    // Assert: NMI enters first, does not reenter, and leaves the IRQ flag pending.
    assert_eq!(machine.step().unwrap().pc_before, 0xfdbc);
    let interrupted_sp = machine.registers().sp;
    machine.cpu_mut().registers.pc = 0x2000;
    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    assert_eq!(machine.step().unwrap().pc_before, 0x2000);
    assert_eq!(machine.registers().sp, interrupted_sp);
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0x80);

    // Act: Clear the NMI flag through its data register.
    machine.read_byte(0x878c).unwrap();
    // Assert: The pending IRQ can now enter its handler.
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);

    // Act: Generate a fresh NMI edge.
    machine.signal_pia_control_line(PiaId::DisplayY, PiaControlLine::Ca1, false);
    machine.signal_pia_control_line(PiaId::DisplayY, PiaControlLine::Ca1, true);
    // Assert: NMI is rearmed and enters its handler again.
    assert_eq!(machine.step().unwrap().pc_before, 0xfdbc);
}

// What: Deliver a keyboard press and release to the base-ROM interrupt handler.
// Why: The handler must record the key and clear its key-down state on release.
#[test]
fn base_rom_interrupt_handler_observes_keyboard_press_and_release() {
    // Arrange: Run to the display wait and explicitly unmask CPU interrupts.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.run_steps(100_000).unwrap();
    assert_eq!(machine.registers().pc, 0xc6b4);

    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    // Act: Deliver A and step the ROM handler until its RAM state changes.
    machine.keyboard_press(b'A');
    machine.keyboard_interrupt_tick();
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);
    for _ in 0..4_096 {
        if machine.read_byte(0x0597).unwrap() == 1 && machine.read_byte(0x059c).unwrap() == b'A' {
            break;
        }
        machine.step().unwrap();
    }
    // Assert: The handler records the pressed key and key-down state.
    assert_eq!(machine.read_byte(0x0597).unwrap(), 1);
    assert_eq!(machine.read_byte(0x059c).unwrap(), b'A');

    // Act: Deliver the release and step until the key-down state clears.
    machine.keyboard_release();
    machine.keyboard_interrupt_tick();
    for _ in 0..4_096 {
        if machine.read_byte(0x0597).unwrap() == 0 {
            break;
        }
        machine.step().unwrap();
    }
    // Assert: The last key remains recorded and the PIA interrupt flag is clear.
    assert_eq!(machine.read_byte(0x0597).unwrap(), 0);
    assert_eq!(machine.read_byte(0x059c).unwrap(), b'A');
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
}

// What: Confirms the bank-select register separates selector and expander-address fields.
// Why: Selector 5 must expose its right-backpack ROM while preserving the address-field value.
#[test]
fn bank_select_register_splits_the_rom_select_and_address_fields() {
    let mut machine = Machine::default();

    // Arrange: Encode selector 5 and expander address 5 in the bank-select register.
    machine.write_byte(BANK_SELECT_REGISTER, 0x2d).unwrap();

    // Act: Inspect the decoded fields and first byte selected from the right backpack.
    let first_rom_byte = machine.read_byte(0x8800).unwrap();

    // Assert: Both fields decode correctly and selector 5 returns its source byte.
    assert_eq!(machine.bank_selection(), (5, 5));
    assert_eq!(first_rom_byte, RIGHT_BACKPACK_ROM[0]);
}

// What: Read the overflow ROM for every expander address and try boundary writes.
// Why: Selector 1 ignores the address field and maps its ROM read-only.
#[test]
fn overflow_bank_maps_both_chips_read_only_and_ignores_expander_address() {
    // Arrange: Use a default machine and select each address field in turn.
    let mut machine = Machine::default();
    for bank_address in 0..8 {
        machine
            .write_byte(BANK_SELECT_REGISTER, 0x08 | bank_address)
            .unwrap();
        // Act: Read the complete mapped overflow image.
        // Assert: Every byte matches the source for this address field.
        for (offset, expected) in OVERFLOW_ROM.iter().copied().enumerate() {
            let address = 0x8800 + u16::try_from(offset).unwrap();
            assert_eq!(machine.read_byte(address).unwrap(), expected);
        }
    }
    // Act: Attempt writes at the chip and window boundaries.
    // Assert: Each mapped ROM byte remains unchanged.
    for address in [0x8800, 0x8fff, 0x9000, 0x97ff] {
        let expected = machine.read_byte(address).unwrap();
        machine.write_byte(address, !expected).unwrap();
        assert_eq!(machine.read_byte(address).unwrap(), expected);
    }
    // Act: Return to the base-ROM selector.
    machine.write_byte(BANK_SELECT_REGISTER, 0).unwrap();
    // Assert: The same address now reads the base image.
    assert_eq!(
        machine.read_byte(0x8800).unwrap(),
        BASE_ROM[usize::from(0x8800 - ROM_BASE)]
    );
}

// What: Confirms selector 4 maps every byte of the source left-backpack image read-only.
// Why: ROM code and data must remain at their reference addresses for startup to continue.
#[test]
fn left_backpack_maps_exact_source_bytes_read_only_for_every_expander_address() {
    let mut machine = Machine::default();
    for bank_address in 0..8 {
        // Arrange: Select the left backpack with each expander-address field value.
        machine
            .write_byte(BANK_SELECT_REGISTER, 0x20 | bank_address)
            .unwrap();

        // Act: Read the complete 2 KiB payload from its mapped address range.
        let mapped_bytes = (0..LEFT_BACKPACK_ROM_SIZE)
            .map(|offset| {
                machine
                    .read_byte(0x8800 + u16::try_from(offset).unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // Assert: Selector 4 ignores the address field and maps source bytes verbatim.
        assert_eq!(mapped_bytes.as_slice(), LEFT_BACKPACK_ROM.as_slice());

        // Act: Attempt writes at both ends of the mapped payload.
        for address in [0x8800, 0x8fff] {
            let original = machine.read_byte(address).unwrap();
            machine.write_byte(address, !original).unwrap();

            // Assert: Backpack ROM writes do not alter the mapped byte.
            assert_eq!(machine.read_byte(address).unwrap(), original);
        }
    }
}

// What: Confirms selector 5 maps the complete reference right-backpack image read-only.
// Why: The full 8 KiB payload must occupy the selector window without changing on writes.
#[test]
fn right_backpack_maps_exact_source_bytes_read_only_for_every_expander_address() {
    let mut machine = Machine::default();
    for bank_address in 0..8 {
        // Arrange: Select the right backpack with each expander-address field value.
        machine
            .write_byte(BANK_SELECT_REGISTER, 0x28 | bank_address)
            .unwrap();

        // Act: Read the full image through the selector-5 memory mapping.
        let mapped_bytes = (0..RIGHT_BACKPACK_ROM_SIZE)
            .map(|offset| {
                machine
                    .read_byte(0x8800 + u16::try_from(offset).unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // Assert: Every mapped byte matches the source payload verbatim.
        assert_eq!(mapped_bytes.as_slice(), RIGHT_BACKPACK_ROM.as_slice());

        // Act: Attempt writes at the first and last bytes in the selected window.
        for address in [0x8800, 0xa7ff] {
            let original = machine.read_byte(address).unwrap();
            machine.write_byte(address, !original).unwrap();

            // Assert: Writes leave the right-backpack image unchanged.
            assert_eq!(machine.read_byte(address).unwrap(), original);
        }
    }
}

// What: Confirms selector 6 maps each left ROM-expander slot byte-for-byte and read-only.
// Why: The bank register's address field selects among eight distinct source slot images.
#[test]
fn left_rom_expander_maps_each_slot_read_only() {
    // Arrange: Select each address-field value used by the reference's left-bank reader.
    let mut machine = Machine::default();
    for slot_index in 0..LEFT_ROM_EXPANDER_SLOT_COUNT {
        let slot = u8::try_from(slot_index).unwrap();
        machine
            .write_byte(BANK_SELECT_REGISTER, 0x30 | slot)
            .unwrap();
        assert_eq!(machine.bank_selection(), (6, slot));

        let source_start = slot_index * ROM_EXPANDER_SLOT_SIZE;
        let source_slot =
            &LEFT_ROM_EXPANDER_BANK[source_start..source_start + ROM_EXPANDER_SLOT_SIZE];

        // Act: Read one complete selector window from the selected slot.
        let mapped_bytes = (0..ROM_EXPANDER_SLOT_SIZE)
            .map(|offset| {
                machine
                    .read_byte(0x8800 + u16::try_from(offset).unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // Assert: The bank window exposes the exact source slot in order.
        assert_eq!(mapped_bytes.as_slice(), source_slot);

        // Act: Attempt writes at the first and last byte of this slot.
        for address in [0x8800, 0xa7ff] {
            let original = machine.read_byte(address).unwrap();
            machine.write_byte(address, !original).unwrap();

            // Assert: The selected ROM byte remains unchanged.
            assert_eq!(machine.read_byte(address).unwrap(), original);
        }
    }
}

// What: Confirms selector 7 maps each right ROM-expander slot byte-for-byte and read-only.
// Why: The register's address field selects among eight distinct source slot images.
#[test]
fn right_rom_expander_maps_each_slot_read_only() {
    // Arrange: Select each address-field value used by the reference's right-bank reader.
    let mut machine = Machine::default();
    for slot_index in 0..RIGHT_ROM_EXPANDER_SLOT_COUNT {
        let slot = u8::try_from(slot_index).unwrap();
        machine
            .write_byte(BANK_SELECT_REGISTER, 0x38 | slot)
            .unwrap();
        assert_eq!(machine.bank_selection(), (7, slot));

        let source_start = slot_index * ROM_EXPANDER_SLOT_SIZE;
        let source_slot =
            &RIGHT_ROM_EXPANDER_BANK[source_start..source_start + ROM_EXPANDER_SLOT_SIZE];

        // Act: Read one complete selector window from the selected slot.
        let mapped_bytes = (0..ROM_EXPANDER_SLOT_SIZE)
            .map(|offset| {
                machine
                    .read_byte(0x8800 + u16::try_from(offset).unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // Assert: The bank window exposes the exact source slot in order.
        assert_eq!(mapped_bytes.as_slice(), source_slot);

        // Act: Attempt writes at the first and last byte of this slot.
        for address in [0x8800, 0xa7ff] {
            let original = machine.read_byte(address).unwrap();
            machine.write_byte(address, !original).unwrap();

            // Assert: The selected ROM byte remains unchanged.
            assert_eq!(machine.read_byte(address).unwrap(), original);
        }
    }
}

// What: Read ignored banks for each address field and attempt writes at boundaries.
// Why: Selectors 2 and 3 intentionally expose read-only zero bytes in the window.
#[test]
fn ignored_banks_are_read_only_zero_bytes_for_every_expander_address() {
    // Arrange: Iterate both ignored selectors and all expander address fields.
    let mut machine = Machine::default();
    for selector in [2, 3] {
        for bank_address in 0..8 {
            machine
                .write_byte(BANK_SELECT_REGISTER, (selector << 3) | bank_address)
                .unwrap();
            // Act: Read the entire selected window.
            // Assert: Every byte is zero.
            for address in 0x8800..0xa800 {
                assert_eq!(
                    machine.read_byte(address).unwrap(),
                    0,
                    "empty bank {selector}:{bank_address} at {address:#06x}"
                );
            }
            // Act: Write representative boundary bytes.
            // Assert: The selected bank continues to read as zero.
            for address in [0x8800, 0x9000, 0x97ff, 0x9800, 0xa7ff] {
                machine.write_byte(address, 0xff).unwrap();
                assert_eq!(machine.read_byte(address).unwrap(), 0);
            }
        }
    }
}

// What: Execute an index-register word load from each ignored ROM bank.
// Why: Firmware probing the header should observe a zero word and corresponding flags.
#[test]
fn cpu_word_probes_observe_no_rom_header_in_ignored_banks() {
    // Arrange: Install an extended word load in RAM after reset.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for (address, byte) in [(0x2000, 0xfe), (0x2001, 0x88), (0x2002, 0x00)] {
        machine.write_byte(address, byte).unwrap();
    }
    for selector in [2, 3] {
        machine
            .write_byte(BANK_SELECT_REGISTER, selector << 3)
            .unwrap();
        machine.cpu_mut().registers.pc = 0x2000;
        machine.cpu_mut().registers.ix = 0xffff;

        // Act: Execute the probe for the selected bank.
        assert_eq!(machine.step().unwrap().pc_after, 0x2003);
        // Assert: IX and condition codes reflect a zero word.
        assert_eq!(machine.registers().ix, 0);
        assert_eq!(
            machine.registers().ccr & (cpu::ZERO | cpu::NEGATIVE | cpu::OVERFLOW),
            cpu::ZERO
        );
    }
}

// What: Confirms unmapped selector windows error without changing base-ROM reads.
// Why: Missing bank data must remain visible instead of becoming a fallback.
#[test]
fn unsupported_bank_windows_remain_errors_without_affecting_base_rom() {
    let mut machine = Machine::default();
    for selector in 1..8 {
        // Arrange: Select each non-default ROM bank.
        machine
            .write_byte(BANK_SELECT_REGISTER, selector << 3)
            .unwrap();

        // Act: Read addresses outside the bank window.
        // Assert: They continue to expose the base ROM for every selector.
        for address in [0x8000, 0x877f, 0xa800, 0xffff] {
            assert_eq!(
                machine.read_byte(address).unwrap(),
                BASE_ROM[usize::from(address - ROM_BASE)],
                "base ROM at {address:#06x} with selector {selector}"
            );
        }
        // Selectors 2/3 are ignored; selectors 5/6/7 have full-window ROM mappings.
        if matches!(selector, 2 | 3 | 5 | 6 | 7) {
            continue;
        }
        // Act: Probe bank-window addresses not covered by an implemented image.
        // Assert: Every such read returns the selector and address in its error.
        for address in [0x8800, 0x97ff, 0x9800, 0xa7ff] {
            if selector == 1 && address < 0x9800 {
                continue;
            }
            if selector == 4 && address < 0x9000 {
                continue;
            }
            assert_eq!(
                machine.read_byte(address),
                Err(MachineBusError::UnsupportedRomBank { selector, address })
            );
        }
    }
}

// What: Run a one-instruction budget followed by a zero-instruction budget.
// Why: Host callers rely on the budget limiting CPU progress.
#[test]
fn step_budget_is_a_hard_upper_bound() {
    // Arrange: Place two NOPs in RAM and start at the first one.
    let mut machine = Machine::default();
    machine.write_byte(0x2000, 0x01).unwrap();
    machine.write_byte(0x2001, 0x01).unwrap();
    machine.cpu_mut().registers.pc = 0x2000;

    // Act: Run with a budget of one and then zero.
    // Assert: Exactly one instruction executes and the zero budget leaves PC unchanged.
    assert_eq!(machine.run_steps(1).unwrap(), 1);
    assert_eq!(machine.registers().pc, 0x2001);
    assert_eq!(machine.run_steps(0).unwrap(), 0);
    assert_eq!(machine.registers().pc, 0x2001);
}
