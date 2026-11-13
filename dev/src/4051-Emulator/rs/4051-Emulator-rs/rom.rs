//! ROM images embedded in the binary and their expected sizes and fingerprints.
//!
//! The machine bus maps these byte arrays into the base and selected bank
//! windows; expander images concatenate the slots in address-field order.

pub const BASE_ROM_SIZE: usize = 32 * 1024;
pub const BASE_ROM_FNV1A64: u64 = 0x9344f0b89d357280;
pub const BASE_ROM_SHA256: &str =
    "5b847c350fbd2e19fb609070bf76a929ce4068292a4c6cdc8568e91b2a4df59e";

pub const BASE_ROM: &[u8; BASE_ROM_SIZE] = include_bytes!("roms/base.bin");

pub const OVERFLOW_ROM_SIZE: usize = 4 * 1024;
pub const OVERFLOW_ROM_FNV1A64: u64 = 0x01f9dd9364531af2;
pub const OVERFLOW_ROM_SHA256: &str =
    "17f387108e9acc1e2b976bc86335784affee99f5ee4cc2ca9f347270b97c46dc";

pub const OVERFLOW_ROM: &[u8; OVERFLOW_ROM_SIZE] = include_bytes!("roms/overflow.bin");

/// Selector 4's 2 KiB left-backpack image (`DERROML` in the reference).
pub const LEFT_BACKPACK_ROM_SIZE: usize = 2 * 1024;
pub const LEFT_BACKPACK_ROM_FNV1A64: u64 = 0xb1dc383ab2158059;
pub const LEFT_BACKPACK_ROM_SHA256: &str =
    "cb16cccbf9f9c45ed2e655e786642c07ebf253caff407912f574af145d59624f";

pub const LEFT_BACKPACK_ROM: &[u8; LEFT_BACKPACK_ROM_SIZE] =
    include_bytes!("roms/backpack-left.bin");

/// Selector 5's 8 KiB right-backpack image (`DERROMR` in the reference).
pub const RIGHT_BACKPACK_ROM_SIZE: usize = 8 * 1024;
pub const RIGHT_BACKPACK_ROM_FNV1A64: u64 = 0x9c50825ef0adc325;
pub const RIGHT_BACKPACK_ROM_SHA256: &str =
    "7d2c7ac4888bfd75cd5f56e8d61f69595121183afc81556c876732fd3782c62f";

pub const RIGHT_BACKPACK_ROM: &[u8; RIGHT_BACKPACK_ROM_SIZE] =
    include_bytes!("roms/backpack-right.bin");

pub const ROM_EXPANDER_SLOT_SIZE: usize = 8 * 1024;
pub const LEFT_ROM_EXPANDER_SLOT_COUNT: usize = 8;
pub const LEFT_ROM_EXPANDER_BANK_SIZE: usize =
    ROM_EXPANDER_SLOT_SIZE * LEFT_ROM_EXPANDER_SLOT_COUNT;
pub const LEFT_ROM_EXPANDER_BANK_FNV1A64: u64 = 0xa81f808d9880651e;
pub const LEFT_ROM_EXPANDER_BANK_SHA256: &str =
    "2f7137f745039e043a004b38e0f814788d8c9951d43f88108b66371859838e40";

/// Selector 6's left-bank slots in the order selected by the register address field.
pub const LEFT_ROM_EXPANDER_BANK: &[u8; LEFT_ROM_EXPANDER_BANK_SIZE] =
    include_bytes!("roms/romexp-left.bin");

pub const RIGHT_ROM_EXPANDER_SLOT_COUNT: usize = 8;
pub const RIGHT_ROM_EXPANDER_BANK_SIZE: usize =
    ROM_EXPANDER_SLOT_SIZE * RIGHT_ROM_EXPANDER_SLOT_COUNT;
pub const RIGHT_ROM_EXPANDER_BANK_FNV1A64: u64 = 0xf464189c814ef54e;
pub const RIGHT_ROM_EXPANDER_BANK_SHA256: &str =
    "d900890f3c71116bfa2a4430ccc63e7c89383143464253f9c3bc6f439b5ce0c3";

/// Selector 7's right-bank slots in the order selected by the register address field.
pub const RIGHT_ROM_EXPANDER_BANK: &[u8; RIGHT_ROM_EXPANDER_BANK_SIZE] =
    include_bytes!("roms/romexp-right.bin");
