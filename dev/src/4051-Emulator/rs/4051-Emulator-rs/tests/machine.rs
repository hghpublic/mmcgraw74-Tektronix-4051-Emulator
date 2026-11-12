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

#[test]
fn reset_loads_the_base_rom_vector() {
    let mut machine = Machine::default();
    machine.reset().unwrap();

    assert_eq!(machine.registers().pc, 0xbc4b);
    assert_eq!(machine.registers().ccr, 0xd0);
}

#[test]
fn base_rom_executes_its_reset_initialization_sequence() {
    let mut machine = Machine::default();
    machine.reset().unwrap();

    assert_eq!(machine.run_steps(12).unwrap(), 12);

    assert_eq!(machine.registers().pc, 0xbc6b);
    assert_eq!(machine.registers().sp, 0x0028);
    assert_eq!(machine.registers().ix, 0x7ff5);
    assert_eq!(machine.read_byte(0x0008).unwrap(), 0);
    assert_eq!(machine.read_byte(0x7fff).unwrap(), 0);
}

#[test]
fn base_rom_waits_for_display_readiness_with_interrupts_masked() {
    let mut machine = Machine::default();
    machine.reset().unwrap();

    assert_eq!(machine.run_steps(100_000).unwrap(), 100_000);
    assert_eq!(machine.registers().pc, 0xc6b4);
    assert_ne!(machine.registers().ccr & cpu::INTERRUPT_MASK, 0);

    let loop_registers = machine.registers();
    assert_eq!(machine.run_steps(12).unwrap(), 12);
    assert_eq!(machine.registers(), loop_registers);
}

#[test]
fn display_erase_requires_a_data_output_falling_edge_and_host_completion() {
    let mut machine = Machine::default();
    machine.set_pia_input(PiaId::DisplayX, PiaPort::A, 0xa0);
    machine.display_ready();

    machine.write_byte(0x8794, 0x10).unwrap();
    machine.write_byte(0x8794, 0).unwrap();
    assert!(!machine.display_erase_pending());
    machine.write_byte(0x8795, 0x04).unwrap();
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa4);

    machine.write_byte(0x8794, 0x10).unwrap();
    assert!(!machine.display_erase_pending());
    machine.write_byte(0x8797, 0x04).unwrap();
    machine.write_byte(0x8796, 0x10).unwrap();
    machine.write_byte(0x8796, 0).unwrap();
    assert!(!machine.display_erase_pending());

    machine.write_byte(0x8794, 0).unwrap();
    assert!(machine.display_erase_pending());
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa0);
    assert!(machine.display_erase_pending());

    machine.display_ready();
    assert!(!machine.display_erase_pending());
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa4);
    machine.write_byte(0x8794, 0).unwrap();
    assert!(!machine.display_erase_pending());
    assert_eq!(machine.read_byte(0x8794).unwrap(), 0xa4);
}

// What: Confirms startup probes both backpacks and the first slot of each ROM expander.
// Why: Startup progress must come from mapped reference bytes, not a CPU-state override.
#[test]
fn startup_probes_backpacks_and_both_expanders() {
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
}

#[test]
fn ram_is_writable_rom_is_read_only_and_unmapped_io_reads_zero() {
    let mut machine = Machine::default();
    let rom_byte = machine.read_byte(0x8000).unwrap();

    machine.write_byte(0x7fff, 0xa5).unwrap();
    machine.write_byte(0x8000, !rom_byte).unwrap();
    machine.write_byte(0x8780, 0xff).unwrap();

    assert_eq!(machine.read_byte(0x7fff).unwrap(), 0xa5);
    assert_eq!(machine.read_byte(0x8000).unwrap(), rom_byte);
    assert_eq!(machine.read_byte(0x8780).unwrap(), 0);
}

#[test]
fn pia_data_register_muxes_input_and_output_bits_by_direction() {
    let mut machine = Machine::default();
    machine
        .write_byte(0x878c, 0x0f)
        .expect("PIA writes cannot fail");
    assert_eq!(machine.read_byte(0x878c).unwrap(), 0x0f);

    machine
        .write_byte(0x878d, 0xff)
        .expect("PIA writes cannot fail");
    machine
        .write_byte(0x878c, 0x0c)
        .expect("PIA writes cannot fail");
    machine.set_pia_input(PiaId::DisplayY, PiaPort::A, 0xa5);

    assert_eq!(machine.read_byte(0x878d).unwrap(), 0x3f);
    assert_eq!(machine.read_byte(0x878c).unwrap(), 0xac);
}

#[test]
fn keyboard_ports_expose_key_release_modifiers_and_caps_lock() {
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();

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

    machine.keyboard_release();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    machine.toggle_caps_lock();
    machine.keyboard_press(b'B');
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x42);
}

#[test]
fn pia_control_edges_latch_status_and_data_reads_clear_it() {
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x07).unwrap();

    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, true);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x87);

    machine.read_byte(0x87a8).unwrap();
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x07);

    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, false);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x07);

    machine.write_byte(0x87a9, 0x05).unwrap();
    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, true);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x05);
    machine.signal_pia_control_line(PiaId::Keyboard, PiaControlLine::Ca1, false);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x85);
}

#[test]
fn board_interrupt_flags_reach_rom_vectors_without_pia_enable_gating() {
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
            let mut machine = Machine::default();
            machine.reset().unwrap();
            machine.cpu_mut().registers.pc = 0x2000;
            if vector == 0xefb8 {
                machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
            }
            machine.write_byte(base + offset, control).unwrap();
            machine.signal_pia_control_line(pia, line, true);

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

#[test]
fn cpu_mask_defers_keyboard_irq_until_unmasked() {
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.cpu_mut().registers.pc = 0x2000;
    machine.write_byte(0x2000, 0x01).unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.keyboard_interrupt_tick();

    assert_eq!(machine.step().unwrap().pc_before, 0x2000);
    assert_eq!(machine.read_byte(0x87a9).unwrap(), 0x84);

    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);
}

#[test]
fn held_nmi_takes_priority_over_irq_and_rearms_after_clearing() {
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    machine.write_byte(0x2000, 0x01).unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.keyboard_interrupt_tick();
    machine.write_byte(0x878d, 0x06).unwrap();
    machine.signal_pia_control_line(PiaId::DisplayY, PiaControlLine::Ca1, true);

    assert_eq!(machine.step().unwrap().pc_before, 0xfdbc);
    let interrupted_sp = machine.registers().sp;
    machine.cpu_mut().registers.pc = 0x2000;
    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    assert_eq!(machine.step().unwrap().pc_before, 0x2000);
    assert_eq!(machine.registers().sp, interrupted_sp);
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0x80);

    machine.read_byte(0x878c).unwrap();
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);

    machine.signal_pia_control_line(PiaId::DisplayY, PiaControlLine::Ca1, false);
    machine.signal_pia_control_line(PiaId::DisplayY, PiaControlLine::Ca1, true);
    assert_eq!(machine.step().unwrap().pc_before, 0xfdbc);
}

#[test]
fn base_rom_interrupt_handler_observes_keyboard_press_and_release() {
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.run_steps(100_000).unwrap();
    assert_eq!(machine.registers().pc, 0xc6b4);

    machine.cpu_mut().registers.ccr &= !cpu::INTERRUPT_MASK;
    machine.keyboard_press(b'A');
    machine.keyboard_interrupt_tick();
    assert_eq!(machine.step().unwrap().pc_before, 0xefb8);
    for _ in 0..4_096 {
        if machine.read_byte(0x0597).unwrap() == 1 && machine.read_byte(0x059c).unwrap() == b'A' {
            break;
        }
        machine.step().unwrap();
    }
    assert_eq!(machine.read_byte(0x0597).unwrap(), 1);
    assert_eq!(machine.read_byte(0x059c).unwrap(), b'A');

    machine.keyboard_release();
    machine.keyboard_interrupt_tick();
    for _ in 0..4_096 {
        if machine.read_byte(0x0597).unwrap() == 0 {
            break;
        }
        machine.step().unwrap();
    }
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

#[test]
fn overflow_bank_maps_both_chips_read_only_and_ignores_expander_address() {
    let mut machine = Machine::default();
    for bank_address in 0..8 {
        machine
            .write_byte(BANK_SELECT_REGISTER, 0x08 | bank_address)
            .unwrap();
        for (offset, expected) in OVERFLOW_ROM.iter().copied().enumerate() {
            let address = 0x8800 + u16::try_from(offset).unwrap();
            assert_eq!(machine.read_byte(address).unwrap(), expected);
        }
    }
    for address in [0x8800, 0x8fff, 0x9000, 0x97ff] {
        let expected = machine.read_byte(address).unwrap();
        machine.write_byte(address, !expected).unwrap();
        assert_eq!(machine.read_byte(address).unwrap(), expected);
    }
    machine.write_byte(BANK_SELECT_REGISTER, 0).unwrap();
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

#[test]
fn ignored_banks_are_read_only_zero_bytes_for_every_expander_address() {
    let mut machine = Machine::default();
    for selector in [2, 3] {
        for bank_address in 0..8 {
            machine
                .write_byte(BANK_SELECT_REGISTER, (selector << 3) | bank_address)
                .unwrap();
            for address in 0x8800..0xa800 {
                assert_eq!(
                    machine.read_byte(address).unwrap(),
                    0,
                    "empty bank {selector}:{bank_address} at {address:#06x}"
                );
            }
            for address in [0x8800, 0x9000, 0x97ff, 0x9800, 0xa7ff] {
                machine.write_byte(address, 0xff).unwrap();
                assert_eq!(machine.read_byte(address).unwrap(), 0);
            }
        }
    }
}

#[test]
fn cpu_word_probes_observe_no_rom_header_in_ignored_banks() {
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

        assert_eq!(machine.step().unwrap().pc_after, 0x2003);
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

#[test]
fn step_budget_is_a_hard_upper_bound() {
    let mut machine = Machine::default();
    machine.write_byte(0x2000, 0x01).unwrap();
    machine.write_byte(0x2001, 0x01).unwrap();
    machine.cpu_mut().registers.pc = 0x2000;

    assert_eq!(machine.run_steps(1).unwrap(), 1);
    assert_eq!(machine.registers().pc, 0x2001);
    assert_eq!(machine.run_steps(0).unwrap(), 0);
    assert_eq!(machine.registers().pc, 0x2001);
}
