//! A small, UI-independent MC6800 execution core.
//!
//! The CPU owns only its architectural registers. [`MemoryBus`] supplies every
//! opcode, operand, vector, stack byte, and memory-mapped access. This keeps
//! instruction execution reusable with either a small test bus or a complete
//! machine implementation.
//!
//! [`Cpu6800::step`] first offers opcodes to shared instruction-family decoders,
//! then handles the remaining supported opcodes explicitly. The implemented
//! instruction set is intentionally staged: an opcode not handled by either
//! route returns [`CpuError::UnsupportedOpcode`] instead of behaving like a
//! no-op. Bus errors are surfaced as they occur; instructions are not generally
//! transactional, so earlier fetches or writes may already have taken effect.
use std::error::Error;
use std::fmt;

// The reset, hardware interrupt, software interrupt, and non-maskable
// interrupt vectors occupy fixed words at the top of the address space.
const RESET_VECTOR: u16 = 0xfffe;
const IRQ_VECTOR: u16 = 0xfff8;
const SWI_VECTOR: u16 = 0xfffa;
const NMI_VECTOR: u16 = 0xfffc;
const RESET_CCR: u8 = 0xc0 | INTERRUPT_MASK;

// Condition-code register bits used by arithmetic, branches, and interrupt
// masking. Keeping the masks named makes flag updates composable.
pub const HALF_CARRY: u8 = 0x20;
pub const INTERRUPT_MASK: u8 = 0x10;
pub const NEGATIVE: u8 = 0x08;
pub const ZERO: u8 = 0x04;
pub const OVERFLOW: u8 = 0x02;
pub const CARRY: u8 = 0x01;

/// Byte-addressable memory and memory-mapped I/O visible to the processor.
///
/// Bus errors are propagated by CPU operations that access memory.
pub trait MemoryBus {
    type Error;

    fn read_byte(&mut self, address: u16) -> Result<u8, Self::Error>;
    fn write_byte(&mut self, address: u16, value: u8) -> Result<(), Self::Error>;
}

/// Architectural registers exposed for inspection and controlled initialization.
///
/// `ir` contains the last opcode fetched by [`Cpu6800::step`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuRegisters {
    pub a: u8,
    pub b: u8,
    pub ix: u16,
    pub pc: u16,
    pub sp: u16,
    pub ccr: u8,
    pub ir: u8,
}

/// A staged MC6800 implementation; unsupported opcodes are returned as errors.
///
/// Register state is public so callers can inspect execution and initialize
/// test or machine state. Instruction coverage is intentionally incomplete.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cpu6800 {
    pub registers: CpuRegisters,
}

/// The result of one successfully executed instruction.
///
/// `pc_before` is the opcode address; `pc_after` is the resulting PC, which
/// may be a branch or jump target rather than the next sequential address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepOutcome {
    pub pc_before: u16,
    pub opcode: u8,
    pub pc_after: u16,
}

/// An error encountered while fetching, executing, or accessing the bus.
///
/// Bus failures are not guaranteed to roll back state already changed by the
/// instruction. Unsupported-opcode errors report the address of the opcode.
#[derive(Debug, PartialEq, Eq)]
pub enum CpuError<E> {
    Bus(E),
    UnsupportedOpcode { address: u16, opcode: u8 },
}

impl<E: fmt::Display> fmt::Display for CpuError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(error) => write!(formatter, "memory bus error: {error}"),
            Self::UnsupportedOpcode { address, opcode } => {
                write!(
                    formatter,
                    "unsupported opcode {opcode:#04x} at {address:#06x}"
                )
            }
        }
    }
}

impl<E: Error + 'static> Error for CpuError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Bus(error) => Some(error),
            Self::UnsupportedOpcode { .. } => None,
        }
    }
}

impl Cpu6800 {
    /// Loads the reset vector and initializes the remaining registers.
    ///
    /// The two vector bytes are read before the register state is replaced, so
    /// a bus error while reading the vector leaves the CPU registers unchanged.
    pub fn reset<B: MemoryBus>(&mut self, bus: &mut B) -> Result<(), CpuError<B::Error>> {
        let high = bus.read_byte(RESET_VECTOR).map_err(CpuError::Bus)?;
        let low = bus
            .read_byte(RESET_VECTOR.wrapping_add(1))
            .map_err(CpuError::Bus)?;

        self.registers = CpuRegisters {
            pc: u16::from_be_bytes([high, low]),
            ccr: RESET_CCR,
            ..CpuRegisters::default()
        };
        Ok(())
    }

    /// Services a maskable IRQ unless the interrupt-mask flag is set.
    ///
    /// Returns `Ok(false)` without changing CPU state when masked. A serviced
    /// interrupt saves the current state, sets the mask, and loads the IRQ
    /// vector.
    pub fn service_irq<B: MemoryBus>(&mut self, bus: &mut B) -> Result<bool, CpuError<B::Error>> {
        if self.flag_is_set(INTERRUPT_MASK) {
            return Ok(false);
        }
        self.enter_interrupt(bus, IRQ_VECTOR)?;
        Ok(true)
    }

    /// Services an NMI regardless of the interrupt-mask flag.
    ///
    /// The current state is saved and the interrupt-mask flag is set before
    /// execution continues at the NMI vector.
    pub fn service_nmi<B: MemoryBus>(&mut self, bus: &mut B) -> Result<(), CpuError<B::Error>> {
        self.enter_interrupt(bus, NMI_VECTOR)
    }

    /// Fetches and executes one instruction at the current program counter.
    ///
    /// The opcode fetch updates `ir` and advances `pc`; later bus errors may
    /// therefore leave partial instruction progress. Unsupported opcodes are
    /// returned as [`CpuError::UnsupportedOpcode`].
    pub fn step<B: MemoryBus>(&mut self, bus: &mut B) -> Result<StepOutcome, CpuError<B::Error>> {
        let pc_before = self.registers.pc;
        let opcode = self.fetch_byte(bus)?;
        self.registers.ir = opcode;

        // These decoders cover opcode families whose high nibble selects both
        // the register and addressing mode. Returning false lets the next
        // decoder try; only after all families decline does the explicit
        // opcode table below decide whether the instruction is unsupported.
        let handled = self.execute_word_group(bus, opcode)?
            || self.execute_memory_group(bus, opcode)?
            || self.execute_unary_group(bus, opcode)?
            || self.execute_inherent(bus, opcode)?;

        if !handled {
            match opcode {
                // Both encodings are modeled as no-ops by this core.
                0x00 | 0x01 => {}
                // Relative branches all consume one signed displacement byte.
                0x20 | 0x22..=0x2f => {
                    // The offset is measured from the PC after its own byte.
                    let displacement = self.fetch_byte(bus)? as i8;
                    if self.branch_condition(opcode) {
                        self.registers.pc = self
                            .registers
                            .pc
                            .wrapping_add_signed(i16::from(displacement));
                    }
                }
                // RTS returns through the two-byte PC previously pushed by BSR.
                0x39 => {
                    self.registers.pc = self.pop_word(bus)?;
                }
                // Immediate A instructions share the same arithmetic and flag
                // helpers; compare forms intentionally discard the byte result.
                0x80 => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.a = self.subtract8(self.registers.a, operand, 0);
                }
                0x81 => {
                    let operand = self.fetch_byte(bus)?;
                    self.subtract8(self.registers.a, operand, 0);
                }
                0x82 => {
                    let operand = self.fetch_byte(bus)?;
                    let carry = u8::from(self.flag_is_set(CARRY));
                    self.registers.a = self.subtract8(self.registers.a, operand, carry);
                }
                0x84 => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.a &= operand;
                    self.set_logical_flags(self.registers.a);
                }
                0x85 => {
                    let operand = self.fetch_byte(bus)?;
                    self.set_logical_flags(self.registers.a & operand);
                }
                0x86 => {
                    let value = self.fetch_byte(bus)?;
                    self.registers.a = value;
                    self.set_logical_flags(value);
                }
                0x88 => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.a ^= operand;
                    self.set_logical_flags(self.registers.a);
                }
                0x89 => {
                    let operand = self.fetch_byte(bus)?;
                    let carry = u8::from(self.flag_is_set(CARRY));
                    self.registers.a = self.add8(self.registers.a, operand, carry);
                }
                0x8a => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.a |= operand;
                    self.set_logical_flags(self.registers.a);
                }
                0x8b => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.a = self.add8(self.registers.a, operand, 0);
                }
                0x8d => {
                    let displacement = self.fetch_byte(bus)? as i8;
                    // Save the address after the displacement so RTS resumes
                    // at the instruction following this call.
                    let return_address = self.registers.pc;
                    self.push_word(bus, return_address)?;
                    self.registers.pc = self
                        .registers
                        .pc
                        .wrapping_add_signed(i16::from(displacement));
                }
                // The B-register immediate family mirrors A's operations.
                0xc0 => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.b = self.subtract8(self.registers.b, operand, 0);
                }
                0xc1 => {
                    let operand = self.fetch_byte(bus)?;
                    self.subtract8(self.registers.b, operand, 0);
                }
                0xc2 => {
                    let operand = self.fetch_byte(bus)?;
                    let carry = u8::from(self.flag_is_set(CARRY));
                    self.registers.b = self.subtract8(self.registers.b, operand, carry);
                }
                0xc4 => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.b &= operand;
                    self.set_logical_flags(self.registers.b);
                }
                0xc5 => {
                    let operand = self.fetch_byte(bus)?;
                    self.set_logical_flags(self.registers.b & operand);
                }
                0xc6 => {
                    self.registers.b = self.fetch_byte(bus)?;
                    self.set_logical_flags(self.registers.b);
                }
                0xc8 => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.b ^= operand;
                    self.set_logical_flags(self.registers.b);
                }
                0xc9 => {
                    let operand = self.fetch_byte(bus)?;
                    let carry = u8::from(self.flag_is_set(CARRY));
                    self.registers.b = self.add8(self.registers.b, operand, carry);
                }
                0xca => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.b |= operand;
                    self.set_logical_flags(self.registers.b);
                }
                0xcb => {
                    let operand = self.fetch_byte(bus)?;
                    self.registers.b = self.add8(self.registers.b, operand, 0);
                }
                0xb7 => {
                    let address = self.fetch_word(bus)?;
                    bus.write_byte(address, self.registers.a)
                        .map_err(CpuError::Bus)?;
                    self.set_flag(OVERFLOW, false);
                    self.set_nz(self.registers.a);
                }
                _ => {
                    return Err(CpuError::UnsupportedOpcode {
                        address: pc_before,
                        opcode,
                    });
                }
            }
        }

        Ok(StepOutcome {
            pc_before,
            opcode,
            pc_after: self.registers.pc,
        })
    }

    fn execute_word_group<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        opcode: u8,
    ) -> Result<bool, CpuError<B::Error>> {
        // The high nibble selects immediate, direct, indexed, or extended
        // addressing and the register context; the low nibble selects the word
        // operation. This decoder declines unsupported combinations.
        let group = opcode >> 4;
        let mode = match group {
            0x8 | 0xc => 0,
            0x9 | 0xd => 1,
            0xa | 0xe => 2,
            0xb | 0xf => 3,
            _ => return Ok(false),
        };
        let operation = opcode & 0x0f;
        let supported = match operation {
            0xc => group <= 0xb,
            0xd => (0x9..=0xb).contains(&group),
            0xe => true,
            0xf => group != 0x8 && group != 0xc,
            _ => false,
        };
        if !supported {
            return Ok(false);
        }

        // JSR saves the address after its operand before transferring control.
        if operation == 0xd {
            let address = self.fetch_address(bus, mode)?;
            let return_address = self.registers.pc;
            self.push_word(bus, return_address)?;
            self.registers.pc = address;
            return Ok(true);
        }

        // STS/STX write the selected 16-bit register in bus byte order.
        if operation == 0xf {
            let address = self.fetch_address(bus, mode)?;
            let value = if group <= 0xb {
                self.registers.sp
            } else {
                self.registers.ix
            };
            self.write_word(bus, address, value)?;
            self.set_flag(OVERFLOW, false);
            self.set_nz16(value);
            return Ok(true);
        }

        // Loads and compares share operand acquisition, but only the load
        // branch below changes the selected word register.
        let value = if mode == 0 {
            self.fetch_word(bus)?
        } else {
            let address = self.fetch_address(bus, mode)?;
            self.read_word(bus, address)?
        };
        match operation {
            0xc => {
                let result = self.registers.ix.wrapping_sub(value);
                self.set_flag(ZERO, result == 0);
                self.set_flag(NEGATIVE, result & 0x8000 != 0);
                // The web reference uses the subtraction borrow as CPX overflow.
                self.set_flag(OVERFLOW, self.registers.ix < value);
            }
            0xe => {
                if group <= 0xb {
                    self.registers.sp = value;
                } else {
                    self.registers.ix = value;
                }
                self.set_flag(OVERFLOW, false);
                self.set_nz16(value);
            }
            _ => unreachable!(),
        }
        Ok(true)
    }

    fn fetch_address<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        mode: u8,
    ) -> Result<u16, CpuError<B::Error>> {
        // Direct addresses are zero-extended bytes; indexed addresses add a
        // zero-extended offset to IX; extended addresses are fetched as words.
        match mode {
            1 => Ok(u16::from(self.fetch_byte(bus)?)),
            2 => Ok(self
                .registers
                .ix
                .wrapping_add(u16::from(self.fetch_byte(bus)?))),
            _ => self.fetch_word(bus),
        }
    }

    fn read_word<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        address: u16,
    ) -> Result<u16, CpuError<B::Error>> {
        // The processor stores words big-endian. Wrapping the second address
        // preserves the 16-bit bus behavior at the end of memory.
        let high = bus.read_byte(address).map_err(CpuError::Bus)?;
        let low = bus
            .read_byte(address.wrapping_add(1))
            .map_err(CpuError::Bus)?;
        Ok(u16::from_be_bytes([high, low]))
    }

    fn write_word<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        address: u16,
        value: u16,
    ) -> Result<(), CpuError<B::Error>> {
        // Match read_word's big-endian bus order; a failing second write does
        // not undo a first byte that the bus has already accepted.
        let [high, low] = value.to_be_bytes();
        bus.write_byte(address, high).map_err(CpuError::Bus)?;
        bus.write_byte(address.wrapping_add(1), low)
            .map_err(CpuError::Bus)
    }

    fn enter_interrupt<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        vector: u16,
    ) -> Result<(), CpuError<B::Error>> {
        // Build the return frame in the order expected by RTI. The mask is set
        // before fetching the vector; a bus failure can leave this frame and
        // mask in place, which callers must treat as partial progress.
        self.push_word(bus, self.registers.pc)?;
        self.push_word(bus, self.registers.ix)?;
        self.push_byte(bus, self.registers.a)?;
        self.push_byte(bus, self.registers.b)?;
        self.push_byte(bus, self.registers.ccr)?;
        self.set_flag(INTERRUPT_MASK, true);
        self.registers.pc = self.read_word(bus, vector)?;
        Ok(())
    }

    fn execute_unary_group<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        opcode: u8,
    ) -> Result<bool, CpuError<B::Error>> {
        // Opcodes 4x/5x operate on accumulators; 6x/7x address memory through
        // IX or an extended word. JMP is the memory-family control-flow case.
        let group = opcode >> 4;
        let operation = opcode & 0x0f;
        let supported_operation =
            matches!(operation, 0x0 | 0x3 | 0x4 | 0x6..=0xa | 0xc | 0xd | 0xf)
                || (matches!(group, 0x6 | 0x7) && operation == 0xe);
        if !matches!(group, 0x4..=0x7) || !supported_operation {
            return Ok(false);
        }

        if group == 0x6 || group == 0x7 {
            let address = if group == 0x6 {
                self.registers
                    .ix
                    .wrapping_add(u16::from(self.fetch_byte(bus)?))
            } else {
                self.fetch_word(bus)?
            };
            if operation == 0xe {
                self.registers.pc = address;
                return Ok(true);
            }

            let value = bus.read_byte(address).map_err(CpuError::Bus)?;
            let old_ccr = self.registers.ccr;
            let (result, write_result) = self.unary_result(operation, value);
            if write_result {
                if let Err(error) = bus.write_byte(address, result) {
                    // A failed memory write must not leave flags describing a
                    // result that never reached the addressed byte.
                    self.registers.ccr = old_ccr;
                    return Err(CpuError::Bus(error));
                }
            }
            return Ok(true);
        }

        let value = if group == 0x4 {
            self.registers.a
        } else {
            self.registers.b
        };
        let (result, _) = self.unary_result(operation, value);
        if group == 0x4 {
            self.registers.a = result;
        } else {
            self.registers.b = result;
        }
        Ok(true)
    }

    fn unary_result(&mut self, operation: u8, value: u8) -> (u8, bool) {
        let old_carry = self.flag_is_set(CARRY);
        // Compute the byte result first, then update flags according to the
        // individual instruction. Shifts/rotates need the old carry as input.
        let result = match operation {
            0x0 => value.wrapping_neg(),
            0x3 => !value,
            0x4 => value >> 1,
            0x6 => (value >> 1) | (u8::from(old_carry) << 7),
            0x7 => (value >> 1) | (value & 0x80),
            0x8 => value << 1,
            0x9 => (value << 1) | u8::from(old_carry),
            0xa => value.wrapping_sub(1),
            0xc => value.wrapping_add(1),
            0xd => value,
            0xf => 0,
            _ => unreachable!(),
        };

        // TST (0xD) is the only supported unary operation that changes flags
        // without writing a result back to its operand.
        match operation {
            0x0 => {
                self.set_flag(OVERFLOW, result == 0x80);
                // Preserve the web reference's result-zero carry behavior.
                self.set_flag(CARRY, result == 0);
                self.set_nz(result);
            }
            0x3 => {
                self.set_flag(OVERFLOW, false);
                self.set_flag(CARRY, true);
                self.set_nz(result);
            }
            0x4 => {
                self.set_flag(CARRY, value & 1 != 0);
                self.set_flag(NEGATIVE, false);
                self.set_flag(ZERO, result == 0);
                self.set_flag(OVERFLOW, self.flag_is_set(CARRY));
            }
            0x6..=0x9 => {
                let carry = if matches!(operation, 0x6 | 0x7) {
                    value & 1 != 0
                } else {
                    value & 0x80 != 0
                };
                self.set_flag(CARRY, carry);
                self.set_nz(result);
                self.set_flag(
                    OVERFLOW,
                    self.flag_is_set(NEGATIVE) ^ self.flag_is_set(CARRY),
                );
            }
            0xa => {
                self.set_flag(OVERFLOW, value == 0x80);
                self.set_nz(result);
            }
            0xc => {
                self.set_flag(OVERFLOW, value == 0x7f);
                self.set_nz(result);
            }
            0xd => {
                self.set_flag(OVERFLOW, false);
                self.set_flag(CARRY, false);
                self.set_nz(value);
            }
            0xf => {
                self.set_flag(NEGATIVE, false);
                self.set_flag(OVERFLOW, false);
                self.set_flag(CARRY, false);
                self.set_flag(ZERO, true);
            }
            _ => unreachable!(),
        }

        (result, operation != 0xd)
    }

    fn execute_inherent<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        opcode: u8,
    ) -> Result<bool, CpuError<B::Error>> {
        match opcode {
            // CCR transfers, IX adjustment, and individual condition-code
            // controls.
            0x06 => self.registers.ccr = (self.registers.a & 0x3f) | 0xc0,
            0x07 => self.registers.a = self.registers.ccr | 0xc0,
            0x08 => {
                self.registers.ix = self.registers.ix.wrapping_add(1);
                self.set_flag(ZERO, self.registers.ix == 0);
            }
            0x09 => {
                self.registers.ix = self.registers.ix.wrapping_sub(1);
                self.set_flag(ZERO, self.registers.ix == 0);
            }
            0x0a => self.set_flag(OVERFLOW, false),
            0x0b => self.set_flag(OVERFLOW, true),
            0x0c => self.set_flag(CARRY, false),
            0x0d => self.set_flag(CARRY, true),
            0x0e => self.set_flag(INTERRUPT_MASK, false),
            0x0f => self.set_flag(INTERRUPT_MASK, true),
            // A/B arithmetic and transfers reuse the same result/flag helpers
            // as the corresponding immediate and memory instructions.
            0x10 => self.registers.a = self.subtract8(self.registers.a, self.registers.b, 0),
            0x11 => {
                self.subtract8(self.registers.a, self.registers.b, 0);
            }
            0x16 => {
                self.registers.b = self.registers.a;
                self.set_logical_flags(self.registers.b);
            }
            0x17 => {
                self.registers.a = self.registers.b;
                self.set_logical_flags(self.registers.a);
            }
            0x1b => self.registers.a = self.add8(self.registers.a, self.registers.b, 0),
            // Stack-pointer/IX transfers and byte stack operations.
            0x30 => self.registers.ix = self.registers.sp.wrapping_add(1),
            0x31 => self.registers.sp = self.registers.sp.wrapping_add(1),
            0x32 => self.registers.a = self.pop_byte(bus)?,
            0x33 => self.registers.b = self.pop_byte(bus)?,
            0x34 => self.registers.sp = self.registers.sp.wrapping_sub(1),
            0x35 => self.registers.sp = self.registers.ix.wrapping_sub(1),
            0x36 => self.push_byte(bus, self.registers.a)?,
            0x37 => self.push_byte(bus, self.registers.b)?,
            0x3b => {
                // RTI consumes the frame created by enter_interrupt. Register
                // state is restored as a unit if any frame read fails.
                let original = self.registers;
                let result = (|| {
                    self.registers.ccr = self.pop_byte(bus)?;
                    self.registers.b = self.pop_byte(bus)?;
                    self.registers.a = self.pop_byte(bus)?;
                    self.registers.ix = self.pop_word(bus)?;
                    self.registers.pc = self.pop_word(bus)?;
                    Ok::<(), CpuError<B::Error>>(())
                })();
                if let Err(error) = result {
                    self.registers = original;
                    return Err(error);
                }
            }
            // SWI shares the hardware interrupt frame but uses its own vector.
            0x3f => self.enter_interrupt(bus, SWI_VECTOR)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn execute_memory_group<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        opcode: u8,
    ) -> Result<bool, CpuError<B::Error>> {
        // A/B accumulator instructions share this decoder: the group chooses
        // accumulator and addressing mode, while the low nibble chooses the
        // arithmetic/logical operation.
        let group = opcode >> 4;
        let mode = match group {
            0x9 | 0xd => 0,
            0xa | 0xe => 1,
            0xb | 0xf => 2,
            _ => return Ok(false),
        };
        let operation = opcode & 0x0f;
        if !matches!(operation, 0x0 | 0x1 | 0x2 | 0x4..=0xb) {
            return Ok(false);
        }

        let address = match mode {
            0 => u16::from(self.fetch_byte(bus)?),
            1 => self
                .registers
                .ix
                .wrapping_add(u16::from(self.fetch_byte(bus)?)),
            _ => self.fetch_word(bus)?,
        };
        let is_b = group >= 0xd;

        if operation == 0x7 {
            // Stores do not need to read the old memory byte.
            let value = if is_b {
                self.registers.b
            } else {
                self.registers.a
            };
            bus.write_byte(address, value).map_err(CpuError::Bus)?;
            self.set_flag(OVERFLOW, false);
            self.set_nz(value);
            return Ok(true);
        }

        let operand = bus.read_byte(address).map_err(CpuError::Bus)?;
        let accumulator = if is_b {
            self.registers.b
        } else {
            self.registers.a
        };
        let value = match operation {
            0x0 => self.subtract8(accumulator, operand, 0),
            0x1 => self.subtract8(accumulator, operand, 0),
            0x2 => self.subtract8(accumulator, operand, u8::from(self.flag_is_set(CARRY))),
            0x4 => {
                let result = accumulator & operand;
                self.set_logical_flags(result);
                result
            }
            0x5 => {
                self.set_logical_flags(accumulator & operand);
                accumulator
            }
            0x6 => {
                self.set_logical_flags(operand);
                operand
            }
            0x8 => {
                let result = accumulator ^ operand;
                self.set_logical_flags(result);
                result
            }
            0x9 => self.add8(accumulator, operand, u8::from(self.flag_is_set(CARRY))),
            0xa => {
                let result = accumulator | operand;
                self.set_logical_flags(result);
                result
            }
            0xb => self.add8(accumulator, operand, 0),
            _ => unreachable!(),
        };

        // Compare operations (1) and bit-test operations (5) update flags but
        // deliberately preserve the accumulator; other operations write back.
        if operation != 0x1 && operation != 0x5 {
            if is_b {
                self.registers.b = value;
            } else {
                self.registers.a = value;
            }
        }
        Ok(true)
    }

    fn fetch_byte<B: MemoryBus>(&mut self, bus: &mut B) -> Result<u8, CpuError<B::Error>> {
        // Advance only after a successful read so a failed fetch leaves PC at
        // the byte that could not be obtained.
        let value = bus.read_byte(self.registers.pc).map_err(CpuError::Bus)?;
        self.registers.pc = self.registers.pc.wrapping_add(1);
        Ok(value)
    }

    fn fetch_word<B: MemoryBus>(&mut self, bus: &mut B) -> Result<u16, CpuError<B::Error>> {
        let high = self.fetch_byte(bus)?;
        let low = self.fetch_byte(bus)?;
        Ok(u16::from_be_bytes([high, low]))
    }

    fn push_byte<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        value: u8,
    ) -> Result<(), CpuError<B::Error>> {
        bus.write_byte(self.registers.sp, value)
            .map_err(CpuError::Bus)?;
        self.registers.sp = self.registers.sp.wrapping_sub(1);
        Ok(())
    }

    fn push_word<B: MemoryBus>(
        &mut self,
        bus: &mut B,
        value: u16,
    ) -> Result<(), CpuError<B::Error>> {
        // Push low then high while SP descends; pop_word consequently reads
        // high then low, matching the processor's stack-frame layout.
        let [high, low] = value.to_be_bytes();
        self.push_byte(bus, low)?;
        self.push_byte(bus, high)
    }

    fn pop_byte<B: MemoryBus>(&mut self, bus: &mut B) -> Result<u8, CpuError<B::Error>> {
        let address = self.registers.sp.wrapping_add(1);
        let value = bus.read_byte(address).map_err(CpuError::Bus)?;
        self.registers.sp = address;
        Ok(value)
    }

    fn pop_word<B: MemoryBus>(&mut self, bus: &mut B) -> Result<u16, CpuError<B::Error>> {
        // If the second byte is unavailable, restore SP so a failed word pop
        // does not consume half of the saved word.
        let original_sp = self.registers.sp;
        let high = self.pop_byte(bus)?;
        let low = match self.pop_byte(bus) {
            Ok(value) => value,
            Err(error) => {
                self.registers.sp = original_sp;
                return Err(error);
            }
        };
        Ok(u16::from_be_bytes([high, low]))
    }

    fn branch_condition(&self, opcode: u8) -> bool {
        let carry = self.flag_is_set(CARRY);
        let negative_overflow_differ = self.flag_is_set(NEGATIVE) ^ self.flag_is_set(OVERFLOW);
        let zero = self.flag_is_set(ZERO);

        // The branch range groups unsigned, single-flag, and signed conditions.
        // Signed ordering uses N xor V because overflow can reverse the result's
        // sign relative to the mathematical ordering of the operands.
        match opcode {
            0x20 => true,
            0x22 => !carry && !zero,
            0x23 => carry || zero,
            0x24 => !carry,
            0x25 => carry,
            0x26 => !zero,
            0x27 => zero,
            0x28 => !self.flag_is_set(OVERFLOW),
            0x29 => self.flag_is_set(OVERFLOW),
            0x2a => !self.flag_is_set(NEGATIVE),
            0x2b => self.flag_is_set(NEGATIVE),
            0x2c => !negative_overflow_differ,
            0x2d => negative_overflow_differ,
            0x2e => !zero && !negative_overflow_differ,
            0x2f => zero || negative_overflow_differ,
            _ => false,
        }
    }

    fn set_nz(&mut self, value: u8) {
        self.set_flag(NEGATIVE, value & 0x80 != 0);
        self.set_flag(ZERO, value == 0);
    }

    fn set_nz16(&mut self, value: u16) {
        self.set_flag(NEGATIVE, value & 0x8000 != 0);
        self.set_flag(ZERO, value == 0);
    }

    fn set_logical_flags(&mut self, value: u8) {
        // Logical results define N/Z and clear V; H, I, and C are preserved.
        self.set_flag(OVERFLOW, false);
        self.set_nz(value);
    }

    fn add8(&mut self, left: u8, right: u8, carry_in: u8) -> u8 {
        // A 16-bit intermediate retains the carry out of an 8-bit sum, while
        // the low byte is the value written back to the accumulator.
        let result = u16::from(left) + u16::from(right) + u16::from(carry_in);
        let value = result as u8;

        // H records carry from the low nibble, V signed-range overflow, and C
        // carry beyond the byte; N/Z describe the truncated result.
        self.set_flag(HALF_CARRY, (u16::from(left ^ right) ^ result) & 0x10 != 0);
        self.set_flag(
            OVERFLOW,
            (u16::from(left ^ right) ^ result ^ (result >> 1)) & 0x80 != 0,
        );
        self.set_flag(CARRY, result & 0x100 != 0);
        self.set_nz(value);
        value
    }

    fn subtract8(&mut self, left: u8, right: u8, carry_in: u8) -> u8 {
        // The wide intermediate retains upper bits for condition-code
        // calculation; only its low byte is written back as the result.
        let result = u16::from(left)
            .wrapping_sub(u16::from(right))
            .wrapping_sub(u16::from(carry_in));
        let value = result as u8;

        // Condition codes use the wide subtraction result; the accumulator
        // receives only its low byte.
        self.set_flag(
            OVERFLOW,
            (u16::from(left ^ right) ^ result ^ (result >> 1)) & 0x80 != 0,
        );
        self.set_flag(CARRY, result & 0x100 != 0);
        self.set_nz(value);
        value
    }

    fn flag_is_set(&self, flag: u8) -> bool {
        self.registers.ccr & flag != 0
    }

    fn set_flag(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.registers.ccr |= flag;
        } else {
            self.registers.ccr &= !flag;
        }
    }
}
