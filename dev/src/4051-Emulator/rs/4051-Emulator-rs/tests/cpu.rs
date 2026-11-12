//! Public-API tests for the staged MC6800 core.
//!
//! Most tests load a short program into a deterministic 64 KiB bus and execute
//! it through [`Cpu6800::step`]; direct interrupt/reset tests use the same bus.
//! Assertions focus on architectural state and memory effects. The bus can
//! reject selected reads or writes to expose partial progress without reaching
//! into CPU internals.
use tek4051_emulator::cpu::{
    CARRY, Cpu6800, CpuError, CpuRegisters, HALF_CARRY, INTERRUPT_MASK, MemoryBus, NEGATIVE,
    OVERFLOW, StepOutcome, ZERO,
};

const RESET_VECTOR: u16 = 0xfffe;
const IRQ_VECTOR: u16 = 0xfff8;
const SWI_VECTOR: u16 = 0xfffa;
const NMI_VECTOR: u16 = 0xfffc;
const RESET_CCR: u8 = 0xd0;

const ADDRESS_SPACE_SIZE: usize = 1 << 16;

// A deterministic 64 KiB memory bus with precise read/write failure injection
// lets tests exercise public CPU behavior without reaching into CPU internals.
struct TestBus {
    memory: Box<[u8; ADDRESS_SPACE_SIZE]>,
    fail_read_at: Option<u16>,
    fail_write_at: Option<u16>,
}

impl Default for TestBus {
    fn default() -> Self {
        Self {
            memory: Box::new([0; ADDRESS_SPACE_SIZE]),
            fail_read_at: None,
            fail_write_at: None,
        }
    }
}

impl TestBus {
    fn load(&mut self, start: u16, bytes: &[u8]) {
        let start = usize::from(start);
        self.memory[start..start + bytes.len()].copy_from_slice(bytes);
    }

    fn set_reset_vector(&mut self, address: u16) {
        self.set_vector(RESET_VECTOR, address);
    }

    fn set_vector(&mut self, vector: u16, address: u16) {
        let [high, low] = address.to_be_bytes();
        self.memory[usize::from(vector)] = high;
        self.memory[usize::from(vector.wrapping_add(1))] = low;
    }
}

impl MemoryBus for TestBus {
    type Error = u16;

    fn read_byte(&mut self, address: u16) -> Result<u8, Self::Error> {
        if self.fail_read_at == Some(address) {
            return Err(address);
        }
        Ok(self.memory[usize::from(address)])
    }

    fn write_byte(&mut self, address: u16, value: u8) -> Result<(), Self::Error> {
        if self.fail_write_at == Some(address) {
            return Err(address);
        }
        self.memory[usize::from(address)] = value;
        Ok(())
    }
}

// What: Verify reset loads the big-endian reset vector and clears stale state.
// Why: Reset must produce a deterministic starting CPU regardless of its prior
// register values.
#[test]
fn reset_loads_the_big_endian_vector_and_initializes_registers() {
    // Arrange: Put a known entry point in memory and dirty every register.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    let mut cpu = Cpu6800 {
        registers: CpuRegisters {
            a: 0xff,
            b: 0xff,
            ix: 0xffff,
            pc: 0x1234,
            sp: 0xffff,
            ccr: 0,
            ir: 0xff,
        },
    };

    // Act: Reset should read the vector before replacing the register set.
    cpu.reset(&mut bus).unwrap();

    // Assert: Only the vector PC and architecture-defined CCR survive reset.
    assert_eq!(
        cpu.registers,
        CpuRegisters {
            pc: 0x2000,
            ccr: 0xd0,
            ..CpuRegisters::default()
        }
    );
}

// What: Execute immediate arithmetic, an extended store, and both accepted NOP bytes.
// Why: The returned step boundaries and resulting memory/register state are the CPU's
// observable contract, not just successful opcode decoding.
#[test]
fn executes_immediate_load_add_extended_store_and_both_nop_encodings() {
    // Arrange: Build a short program whose arithmetic result is stored before the NOPs.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(
        0x2000,
        &[0x86, 0x7f, 0x8b, 0x01, 0xb7, 0x30, 0x00, 0x01, 0x00],
    );
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Execute the load and add separately to check each reported PC boundary.
    assert_eq!(
        cpu.step(&mut bus).unwrap(),
        StepOutcome {
            pc_before: 0x2000,
            opcode: 0x86,
            pc_after: 0x2002,
        }
    );
    assert_eq!(
        cpu.step(&mut bus).unwrap(),
        StepOutcome {
            pc_before: 0x2002,
            opcode: 0x8b,
            pc_after: 0x2004,
        }
    );
    assert_eq!(cpu.registers.a, 0x80);
    assert_eq!(
        cpu.registers.ccr & (NEGATIVE | HALF_CARRY | OVERFLOW),
        NEGATIVE | HALF_CARRY | OVERFLOW
    );
    assert_eq!(cpu.registers.ccr & CARRY, 0);

    // Act: Store the computed value, then run each accepted NOP encoding.
    cpu.step(&mut bus).unwrap();
    assert_eq!(bus.memory[0x3000], 0x80);
    assert_eq!(cpu.registers.pc, 0x2007);

    // Assert: NOPs advance PC and IR without changing the other registers.
    let before_nop = cpu.registers;
    let nop = cpu.step(&mut bus).unwrap();
    assert_eq!(nop.opcode, 0x01);
    assert_eq!(
        cpu.registers,
        CpuRegisters {
            pc: 0x2008,
            ir: 0x01,
            ..before_nop
        }
    );

    let before_zero_nop = cpu.registers;
    assert_eq!(
        cpu.step(&mut bus).unwrap(),
        StepOutcome {
            pc_before: 0x2008,
            opcode: 0x00,
            pc_after: 0x2009,
        }
    );
    assert_eq!(
        cpu.registers,
        CpuRegisters {
            pc: 0x2009,
            ir: 0x00,
            ..before_zero_nop
        }
    );
}

// What: Add 1 to 0xff and inspect the resulting accumulator and flags.
// Why: This boundary exercises byte wrap, carry-out, and carry from the low nibble together.
#[test]
fn add_sets_zero_carry_and_half_carry_at_byte_boundary() {
    // Arrange: Load 0xff and make the next instruction add one.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0xff, 0x8b, 0x01]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Run the load and overflowing addition.
    cpu.step(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();

    // Assert: The byte wraps to zero with both carry indicators set.
    assert_eq!(cpu.registers.a, 0);
    assert_eq!(
        cpu.registers.ccr & (ZERO | CARRY | HALF_CARRY),
        ZERO | CARRY | HALF_CARRY
    );
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), 0);
}

// What: Run SUB, SBC, and CMP in sequence on accumulator A.
// Why: SBC must consume SUB's borrow, and CMP must leave the accumulator untouched.
#[test]
fn subtract_compare_and_subtract_with_carry_update_accumulator_and_flags() {
    // Arrange: Set A to zero and provide operands that produce borrow and wrap cases.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0x00, 0x80, 0x01, 0x82, 0xfe, 0x81, 0x01]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: SUB underflows once, then SBC consumes the carry left by the preceding result.
    cpu.step(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();
    // Assert: SUB writes the wrapped byte and its negative/borrow flags.
    assert_eq!(cpu.registers.a, 0xff);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY), NEGATIVE | CARRY);
    assert_eq!(cpu.registers.ccr & (ZERO | OVERFLOW), 0);

    // Act: SBC subtracts 0xfe using the borrow produced by SUB.
    cpu.step(&mut bus).unwrap();
    // Assert: The carry-in affects the result and leaves A at zero.
    assert_eq!(cpu.registers.a, 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY | OVERFLOW), 0);

    // Act: CMP compares A with one without storing the difference.
    cpu.step(&mut bus).unwrap();
    // Assert: The compare result updates flags but preserves A.
    assert_eq!(cpu.registers.a, 0);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY), NEGATIVE | CARRY);
    assert_eq!(cpu.registers.ccr & ZERO, 0);
}

// What: Subtract one from the signed minimum byte.
// Why: The result is representable as a byte but not as a signed 8-bit value.
#[test]
fn subtract_detects_signed_overflow() {
    // Arrange: Load 0x80, then subtract 1.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0x80, 0x80, 0x01]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Execute both instructions so the subtraction sees the boundary operand.
    cpu.step(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();

    // Assert: The signed result wraps to 0x7f and sets only overflow among these flags.
    assert_eq!(cpu.registers.a, 0x7f);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY), 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);
}

// What: Compare A without storing, then subtract with carry at a signed boundary.
// Why: Both operations must report signed overflow without corrupting the compare operand.
#[test]
fn compare_and_subtract_with_carry_detect_signed_overflow_without_losing_compare_operand() {
    // Arrange: Start A at the signed minimum and encode CMP followed by SBC.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x81, 0x01, 0x82, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.a = 0x80;

    // Act: CMP computes flags while leaving A untouched.
    cpu.step(&mut bus).unwrap();
    // Assert: The compare operand remains available for the next instruction.
    assert_eq!(cpu.registers.a, 0x80);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY | ZERO), 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);

    // Act: Seed carry-in, then execute SBC against zero.
    cpu.registers.ccr |= CARRY;
    cpu.step(&mut bus).unwrap();
    // Assert: SBC stores the result and retains the signed-overflow indication.
    assert_eq!(cpu.registers.a, 0x7f);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY | ZERO), 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);
}

// What: Exercise CMPB, SUBB, and SBCB around signed overflow.
// Why: The B-register opcode family must preserve compare semantics and honor carry-in like A.
#[test]
fn b_compare_subtract_and_subtract_with_carry_detect_signed_overflow() {
    // Arrange: Begin with B=0x80; the program compares, reloads B, then subtracts.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0xc0, 0x01, 0xc6, 0x80, 0xc1, 0x01, 0xc2, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.b = 0x80;

    // Act: Compare B with one.
    cpu.step(&mut bus).unwrap();
    // Assert: Compare reports overflow without changing B.
    assert_eq!(cpu.registers.b, 0x7f);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY | ZERO), 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);

    // Act: Reload B and compare again to set the condition codes for the next phase.
    cpu.step(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.registers.b, 0x80);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY | ZERO), 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);

    // Act: Set carry-in and execute SBCB.
    cpu.registers.ccr |= CARRY;
    cpu.step(&mut bus).unwrap();
    // Assert: B receives the carry-adjusted result and overflow remains visible.
    assert_eq!(cpu.registers.b, 0x7f);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY | ZERO), 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);
}

// What: Load and store A/B through direct, indexed, and extended addresses.
// Why: Each encoding derives its effective address differently but must access
// the intended byte.
#[test]
fn direct_indexed_and_extended_loads_and_stores_use_their_addressing_modes() {
    // Arrange: Give each addressing mode a distinct source byte and destination location.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(
        0x2000,
        &[
            0x96, 0x20, 0xa6, 0x01, 0xb6, 0x30, 0x02, 0xd6, 0x21, 0xe6, 0x02, 0xf6, 0x30, 0x03,
            0x97, 0x22, 0xa7, 0x03, 0xb7, 0x30, 0x04, 0xd7, 0x23, 0xe7, 0x04, 0xf7, 0x30, 0x05,
        ],
    );
    bus.memory[0x20] = 0x11;
    bus.memory[0x21] = 0x44;
    bus.memory[0x1001] = 0x22;
    bus.memory[0x1002] = 0x55;
    bus.memory[0x3002] = 0x33;
    bus.memory[0x3003] = 0x66;
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.ix = 0x1000;

    // Act: Load both accumulators through the three address forms.
    for _ in 0..6 {
        cpu.step(&mut bus).unwrap();
    }
    // Assert: Each load reaches the value stored at its direct/indexed/extended address.
    assert_eq!((cpu.registers.a, cpu.registers.b), (0x33, 0x66));

    // Act: Store the loaded values through the same address forms.
    for _ in 0..6 {
        cpu.step(&mut bus).unwrap();
    }
    // Assert: Every destination contains the accumulator value selected for it.
    assert_eq!(bus.memory[0x22], 0x33);
    assert_eq!(bus.memory[0x1003], 0x33);
    assert_eq!(bus.memory[0x3004], 0x33);
    assert_eq!(bus.memory[0x23], 0x66);
    assert_eq!(bus.memory[0x1004], 0x66);
    assert_eq!(bus.memory[0x3005], 0x66);
}

// What: Mix memory-based addition, comparison, and B subtraction with carry.
// Why: Reads must use the selected addressing mode; compare must not overwrite A.
#[test]
fn memory_compare_and_arithmetic_update_flags_without_corrupting_operands() {
    // Arrange: Seed direct, indexed, and extended operands with distinguishable values.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x9b, 0x30, 0xa1, 0x01, 0xf2, 0x30, 0x02]);
    bus.memory[0x30] = 0x01;
    bus.memory[0x1001] = 0x80;
    bus.memory[0x3002] = 0x01;
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.a = 0x7f;
    cpu.registers.b = 0x02;
    cpu.registers.ix = 0x1000;

    // Act: Add the direct operand to A.
    cpu.step(&mut bus).unwrap();
    // Assert: A and flags reflect the addition's overflow boundary.
    assert_eq!(cpu.registers.a, 0x80);
    assert_eq!(
        cpu.registers.ccr & (NEGATIVE | OVERFLOW | HALF_CARRY),
        NEGATIVE | OVERFLOW | HALF_CARRY
    );

    // Act: Compare A against the indexed operand.
    cpu.step(&mut bus).unwrap();
    // Assert: A remains unchanged while equality is reported.
    assert_eq!(cpu.registers.a, 0x80);
    assert_ne!(cpu.registers.ccr & ZERO, 0);

    // Act: Set carry-in and subtract the extended operand from B.
    cpu.registers.ccr |= CARRY;
    cpu.step(&mut bus).unwrap();
    // Assert: The extended operand and carry-in determine B's result.
    assert_eq!(cpu.registers.b, 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
}

// What: Run accumulator NEG/LSR and memory CLR operations.
// Why: These unary families update different combinations of value, carry, and result flags.
#[test]
fn inherent_and_unary_operations_update_registers_and_condition_codes() {
    // Arrange: The program places an accumulator operation before a memory clear.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0x80, 0x40, 0x44, 0x7f, 0x00, 0x30]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Load 0x80 and negate it, the signed edge case for NEG.
    cpu.step(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();
    // Assert: NEG preserves the high bit and reports its overflow behavior.
    assert_eq!(cpu.registers.a, 0x80);
    assert_eq!(
        cpu.registers.ccr & (NEGATIVE | OVERFLOW | CARRY | ZERO),
        NEGATIVE | OVERFLOW
    );

    // Act: Shift A right, then clear the addressed memory byte.
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.registers.a, 0x40);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW | CARRY | ZERO), 0);

    cpu.step(&mut bus).unwrap();
    // Assert: CLR writes zero and sets the zero flag.
    assert_eq!(bus.memory[0x0030], 0);
    assert_eq!(
        cpu.registers.ccr & (NEGATIVE | OVERFLOW | CARRY | ZERO),
        ZERO
    );
}

// What: Load a 16-bit register, store it to memory, then compare it.
// Why: Immediate operands and extended memory words must use consistent big-endian ordering.
#[test]
fn word_load_store_and_compare_support_immediate_and_extended_modes() {
    // Arrange: Encode LDX immediate, STX extended, and CPX immediate.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(
        0x2000,
        &[0xce, 0x12, 0x34, 0xff, 0x30, 0x00, 0x8c, 0x12, 0x34],
    );
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Load IX, store its word, and compare it with the original operand.
    cpu.step(&mut bus).unwrap();
    // Assert: The word bytes retain big-endian order and CPX reports equality.
    assert_eq!(cpu.registers.ix, 0x1234);
    cpu.step(&mut bus).unwrap();
    assert_eq!(&bus.memory[0x3000..0x3002], &[0x12, 0x34]);
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.registers.ix, 0x1234);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
}

// What: Enter a software interrupt and return with RTI.
// Why: The interrupt frame must preserve the complete caller-visible CPU state.
#[test]
fn software_interrupt_and_rti_restore_the_saved_cpu_state() {
    // Arrange: Point the SWI vector at RTI and seed registers with non-default values.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.set_vector(SWI_VECTOR, 0x3000);
    bus.load(0x2000, &[0x3f]);
    bus.load(0x3000, &[0x3b]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers = CpuRegisters {
        a: 0x12,
        b: 0x34,
        ix: 0x4567,
        pc: 0x2000,
        sp: 0x1000,
        ccr: 0xc5,
        ir: 0,
    };

    // Act: SWI saves the state and transfers control through the vector.
    cpu.step(&mut bus).unwrap();
    // Assert: The handler PC and interrupt mask reflect interrupt entry.
    assert_eq!(cpu.registers.pc, 0x3000);
    assert_ne!(cpu.registers.ccr & INTERRUPT_MASK, 0);
    assert_eq!(cpu.registers.sp, 0x0ff9);

    // Act: RTI consumes the saved frame.
    cpu.step(&mut bus).unwrap();
    // Assert: The original registers, PC, stack pointer, and CCR are restored.
    assert_eq!(cpu.registers.pc, 0x2001);
    assert_eq!(cpu.registers.a, 0x12);
    assert_eq!(cpu.registers.b, 0x34);
    assert_eq!(cpu.registers.ix, 0x4567);
    assert_eq!(cpu.registers.sp, 0x1000);
    assert_eq!(cpu.registers.ccr, 0xc5);
    assert_eq!(cpu.registers.ir, 0x3b);
}

// What: Compare masked IRQ handling with unconditional NMI handling.
// Why: The CPU-level I flag must block only the maskable interrupt path.
#[test]
fn irq_obeys_the_interrupt_mask_while_nmi_does_not() {
    // Arrange: Install distinct vectors and leave reset's interrupt mask set.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.set_vector(IRQ_VECTOR, 0x3000);
    bus.set_vector(NMI_VECTOR, 0x4000);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.sp = 0x1000;

    // Act: Try IRQ while masked, then service NMI without changing the mask.
    assert!(!cpu.service_irq(&mut bus).unwrap());
    // Assert: The masked IRQ has not touched the current control flow or stack.
    assert_eq!(cpu.registers.pc, 0x2000);
    assert_eq!(cpu.registers.sp, 0x1000);

    // Act: NMI must enter its handler despite I being set; then unmask IRQ.
    cpu.service_nmi(&mut bus).unwrap();
    // Assert: NMI uses its vector and saves a frame even though I was set.
    assert_eq!(cpu.registers.pc, 0x4000);
    assert_eq!(cpu.registers.sp, 0x0ff9);

    // Act: With I clear, the same IRQ request is accepted.
    cpu.registers.ccr &= !INTERRUPT_MASK;
    assert!(cpu.service_irq(&mut bus).unwrap());
    // Assert: Execution now uses the IRQ vector.
    assert_eq!(cpu.registers.pc, 0x3000);
}

// What: Fail IRQ entry once during stack writing and once during vector reading.
// Why: Bus errors can happen at different stages, so each failure has distinct partial state.
#[test]
fn irq_bus_failures_are_reported_with_expected_stack_effects() {
    // Arrange: Enable IRQ and fail its first stack write.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.set_vector(IRQ_VECTOR, 0x3000);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.sp = 0x1000;
    cpu.registers.ccr &= !INTERRUPT_MASK;
    bus.fail_write_at = Some(0x1000);

    // Act: Interrupt entry cannot begin when the initial frame byte is rejected.
    assert_eq!(cpu.service_irq(&mut bus), Err(CpuError::Bus(0x1000)));
    // Assert: No vector transfer, stack movement, or interrupt mask update occurred.
    assert_eq!(cpu.registers.pc, 0x2000);
    assert_eq!(cpu.registers.sp, 0x1000);
    assert_eq!(cpu.registers.ccr & INTERRUPT_MASK, 0);

    // Arrange: Move the failure to vector access after all frame bytes can be written.
    bus.fail_write_at = None;
    bus.fail_read_at = Some(IRQ_VECTOR);
    // Act: IRQ entry now fails only after saving state and setting I.
    assert_eq!(cpu.service_irq(&mut bus), Err(CpuError::Bus(IRQ_VECTOR)));
    // Assert: The frame and mask remain as partial progress; PC has no new vector.
    assert_eq!(cpu.registers.pc, 0x2000);
    assert_eq!(cpu.registers.sp, 0x0ff9);
    assert_ne!(cpu.registers.ccr & INTERRUPT_MASK, 0);
}

// What: Check NEG and CPX condition codes at their reference-specific edges.
// Why: These observed web-core quirks are part of the current compatibility target.
#[test]
fn neg_and_cpx_flags_preserve_reference_specific_behavior() {
    // Arrange: Use zero for NEG and compare IX=0 against the word value one.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x40, 0x8c, 0x00, 0x01]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.a = 0;
    cpu.registers.ix = 0;

    // Act: NEG zero, then inspect the carry behavior retained by the reference.
    cpu.step(&mut bus).unwrap();
    // Assert: The result remains zero and the reference-specific carry is set.
    assert_eq!(cpu.registers.a, 0);
    assert_ne!(cpu.registers.ccr & CARRY, 0);

    // Act: Clear carry explicitly before CPX to isolate its overflow behavior.
    cpu.registers.ccr &= !CARRY;
    cpu.step(&mut bus).unwrap();
    // Assert: CPX preserves IX and applies the reference's borrow/overflow flags.
    assert_eq!(cpu.registers.ix, 0);
    assert_eq!(cpu.registers.ccr & CARRY, 0);
    assert_ne!(cpu.registers.ccr & OVERFLOW, 0);
    assert_ne!(cpu.registers.ccr & NEGATIVE, 0);
}

// What: Add the incoming carry to accumulator B.
// Why: ADC differs from ordinary addition only when carry-in participates in the sum.
#[test]
fn b_add_with_carry_consumes_carry_in() {
    // Arrange: Choose 0x7f + carry so the result crosses the signed boundary.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0xc9, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.b = 0x7f;
    cpu.registers.ccr |= CARRY;

    // Act: Execute ADCB with the carry flag already set.
    cpu.step(&mut bus).unwrap();

    // Assert: B receives 0x80 and arithmetic flags describe the carried sum.
    assert_eq!(cpu.registers.b, 0x80);
    assert_eq!(
        cpu.registers.ccr & (NEGATIVE | HALF_CARRY | OVERFLOW),
        NEGATIVE | HALF_CARRY | OVERFLOW
    );
    assert_eq!(cpu.registers.ccr & CARRY, 0);
}

// What: Exercise every implemented conditional branch against selected CCR inputs.
// Why: A one-bit mistake in the condition expression can break signed and unsigned control flow.
#[test]
fn conditional_branches_match_condition_code_combinations() {
    // Arrange: Each row supplies an opcode, initial flags, and expected branch decision.
    let cases = [
        (0x20, 0, true),
        (0x22, 0, true),
        (0x22, CARRY, false),
        (0x22, ZERO, false),
        (0x22, CARRY | ZERO, false),
        (0x23, 0, false),
        (0x23, CARRY, true),
        (0x23, ZERO, true),
        (0x23, CARRY | ZERO, true),
        (0x24, 0, true),
        (0x24, CARRY, false),
        (0x25, 0, false),
        (0x25, CARRY, true),
        (0x26, 0, true),
        (0x26, ZERO, false),
        (0x27, 0, false),
        (0x27, ZERO, true),
        (0x28, 0, true),
        (0x28, OVERFLOW, false),
        (0x29, 0, false),
        (0x29, OVERFLOW, true),
        (0x2a, 0, true),
        (0x2a, NEGATIVE, false),
        (0x2b, 0, false),
        (0x2b, NEGATIVE, true),
        (0x2c, 0, true),
        (0x2c, NEGATIVE, false),
        (0x2c, OVERFLOW, false),
        (0x2c, NEGATIVE | OVERFLOW, true),
        (0x2d, 0, false),
        (0x2d, NEGATIVE, true),
        (0x2d, OVERFLOW, true),
        (0x2d, NEGATIVE | OVERFLOW, false),
        (0x2e, 0, true),
        (0x2e, ZERO, false),
        (0x2e, NEGATIVE, false),
        (0x2e, OVERFLOW, false),
        (0x2e, ZERO | NEGATIVE, false),
        (0x2e, NEGATIVE | OVERFLOW, true),
        (0x2f, 0, false),
        (0x2f, ZERO, true),
        (0x2f, NEGATIVE, true),
        (0x2f, OVERFLOW, true),
        (0x2f, ZERO | NEGATIVE, true),
        (0x2f, NEGATIVE | OVERFLOW, false),
    ];

    for (opcode, flags, taken) in cases {
        // Arrange: Use a fresh CPU per row so one branch cannot affect another case.
        let mut bus = TestBus::default();
        bus.set_reset_vector(0x2000);
        bus.load(0x2000, &[opcode, 0x02]);
        let mut cpu = Cpu6800::default();
        cpu.reset(&mut bus).unwrap();
        cpu.registers.ccr = RESET_CCR | flags;
        let ccr_before = cpu.registers.ccr;

        // Act: Execute the branch with the row's condition-code combination.
        cpu.step(&mut bus).unwrap();

        // Assert: Branch evaluation preserves CCR and chooses the expected PC.
        assert_eq!(cpu.registers.ccr, ccr_before);
        assert_eq!(
            cpu.registers.pc,
            if taken { 0x2004 } else { 0x2002 },
            "branch opcode {opcode:#04x} with flags {flags:#04x}"
        );
    }
}

// What: Verify taken/not-taken branches, signed offsets, and 16-bit PC wraparound.
// Why: Relative addressing depends on the post-operand PC and must wrap at both
// address-space edges.
#[test]
fn relative_branches_apply_signed_offsets_and_wrap_the_program_counter() {
    // Arrange: The first program mixes conditional branches with a backward BRA.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x26, 0x02, 0x01, 0x01, 0x27, 0x02, 0x20, 0xfc]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Traverse the small program, alternating between skipped bytes and targets.
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.registers.pc, 0x2004);
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.registers.pc, 0x2006);
    cpu.step(&mut bus).unwrap();
    // Assert: Taken branches use the post-operand PC; the conditional case skips its target.
    assert_eq!(cpu.registers.pc, 0x2004);

    // Arrange: Place BRA at the top of memory so its positive target wraps to zero.
    let mut wrapping_bus = TestBus::default();
    wrapping_bus.set_reset_vector(0xfffc);
    wrapping_bus.load(0xfffc, &[0x20, 0x02]);
    let mut wrapping_cpu = Cpu6800::default();
    wrapping_cpu.reset(&mut wrapping_bus).unwrap();

    // Act: Execute the branch across the address-space boundary.
    wrapping_cpu.step(&mut wrapping_bus).unwrap();

    // Assert: The 16-bit target wraps instead of overflowing.
    assert_eq!(wrapping_cpu.registers.pc, 0x0000);

    // Arrange: Cover positive/negative displacements at ordinary and wrapped PCs.
    for (start, displacement, expected_pc) in [
        (0x2000, 0x7f, 0x2081),
        (0x2000, 0x80, 0x1f82),
        (0xfffc, 0x7f, 0x007d),
        (0x0000, 0x80, 0xff82),
    ] {
        let mut bus = TestBus::default();
        bus.set_reset_vector(start);
        bus.load(start, &[0x20, displacement]);
        let mut cpu = Cpu6800::default();
        cpu.reset(&mut bus).unwrap();

        // Act: Take BRA so each offset is applied from the address after its operand.
        cpu.step(&mut bus).unwrap();

        // Assert: Signed extension and 16-bit wrapping produce the expected target.
        assert_eq!(cpu.registers.pc, expected_pc);
    }
}

// What: Call a subroutine across a wrapping stack, then return to the saved PC.
// Why: The two-byte frame must preserve byte order and restore SP across address zero.
#[test]
fn branch_to_subroutine_and_return_preserve_word_order_across_stack_wrap() {
    // Arrange: Start with SP at zero and place BSR/RTS around a return-site NOP.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x8d, 0x02, 0x01, 0x01, 0x39]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.ccr |= HALF_CARRY | NEGATIVE | ZERO | OVERFLOW | CARRY;
    let ccr_before = cpu.registers.ccr;

    // Act: BSR saves the return address while its two pushes wrap through memory zero.
    cpu.step(&mut bus).unwrap();

    // Assert: The frame bytes and subroutine target are laid out in stack order.
    assert_eq!(cpu.registers.pc, 0x2004);
    assert_eq!(cpu.registers.ccr, ccr_before);
    assert_eq!(cpu.registers.sp, 0xfffe);
    assert_eq!(bus.memory[0], 0x02);
    assert_eq!(bus.memory[0xffff], 0x20);

    // Act: RTS consumes the wrapped frame.
    cpu.step(&mut bus).unwrap();

    // Assert: Return resumes after BSR and restores the original SP.
    assert_eq!(cpu.registers.pc, 0x2002);
    assert_eq!(cpu.registers.ccr, ccr_before);
    assert_eq!(cpu.registers.sp, 0x0000);
}

// What: Execute BSR with a negative signed displacement.
// Why: The offset must be sign-extended, and the saved return PC must remain
// the post-operand address.
#[test]
fn bsr_sign_extends_negative_displacement_and_saves_the_return_address() {
    // Arrange: Encode -4 and use a known stack location.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x8d, 0xfc]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.sp = 0x0100;

    // Act: Call backward from the address after the displacement byte.
    cpu.step(&mut bus).unwrap();

    // Assert: PC is the signed target and the stack contains the correct return word.
    assert_eq!(cpu.registers.pc, 0x1ffe);
    assert_eq!(cpu.registers.sp, 0x00fe);
    assert_eq!(bus.memory[0x0100], 0x02);
    assert_eq!(bus.memory[0x00ff], 0x20);
}

// What: Inject errors on the low-byte and high-byte writes of a BSR stack frame.
// Why: The test documents which partial memory/SP effects remain after each failed bus write.
#[test]
fn bsr_stack_write_failures_return_the_bus_error_without_branching() {
    // The first-write failure leaves memory untouched; the second leaves the low byte written.
    for (failing_address, expected_sp, expected_low_byte) in
        [(0x0001, 0x0001, 0x00), (0x0000, 0x0000, 0x02)]
    {
        // Arrange: Fail one frame write while keeping the program and initial SP identical.
        let mut bus = TestBus {
            fail_write_at: Some(failing_address),
            ..TestBus::default()
        };
        bus.set_reset_vector(0x2000);
        bus.load(0x2000, &[0x8d, 0x02]);
        let mut cpu = Cpu6800::default();
        cpu.reset(&mut bus).unwrap();
        cpu.registers.sp = 0x0001;

        // Act: Execute BSR until the selected stack write is rejected.
        assert_eq!(cpu.step(&mut bus), Err(CpuError::Bus(failing_address)));
        // Assert: The call does not branch; only writes completed before the error remain.
        assert_eq!(cpu.registers.pc, 0x2002);
        assert_eq!(cpu.registers.sp, expected_sp);
        assert_eq!(bus.memory[0x0001], expected_low_byte);
    }
}

// What: Fail RTS while reading each byte of its return address.
// Why: A failed second pop must undo the first SP advance, while opcode fetch is already complete.
#[test]
fn rts_stack_read_failures_preserve_sp_and_do_not_change_pc_to_return_target() {
    for failing_address in [0xffff, 0x0000] {
        // Arrange: Start at a wrapping stack boundary and fail the selected byte read.
        let mut bus = TestBus::default();
        bus.set_reset_vector(0x2000);
        bus.load(0x2000, &[0x39]);
        let mut cpu = Cpu6800::default();
        cpu.reset(&mut bus).unwrap();
        bus.fail_read_at = Some(failing_address);
        cpu.registers.sp = 0xfffe;
        let registers_before_return = cpu.registers;

        // Act: Execute RTS; its opcode fetch succeeds before stack access fails.
        assert_eq!(cpu.step(&mut bus), Err(CpuError::Bus(failing_address)));
        // Assert: SP is unchanged and no partial return target replaces PC.
        assert_eq!(cpu.registers.pc, 0x2001);
        assert_eq!(cpu.registers.sp, registers_before_return.sp);
        assert_eq!(cpu.registers.ir, 0x39);
    }
}

// What: Run A-register logical instructions followed by ADC with carry-in.
// Why: Logical operations update selected flags but must preserve unrelated CCR bits.
#[test]
fn immediate_a_logical_and_carry_add_operations_match_flag_behavior() {
    // Arrange: Start with a known accumulator and seed flags that logical ops must preserve.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x84, 0x0f, 0x8a, 0x80, 0x85, 0x80, 0x88, 0x80]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.a = 0xf0;
    cpu.registers.ccr |= HALF_CARRY | CARRY | OVERFLOW;

    // Act: ANDA runs first against the immediate mask.
    cpu.step(&mut bus).unwrap();
    // Assert: ANDA updates A/Z but preserves carry and half-carry.
    assert_eq!(cpu.registers.a, 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: ORA sets the high bit.
    cpu.registers.ccr |= OVERFLOW;
    cpu.step(&mut bus).unwrap();
    // Assert: The logical result updates N and clears V without disturbing other flags.
    assert_eq!(cpu.registers.a, 0x80);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), NEGATIVE);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: BITA tests selected bits without writing A.
    cpu.registers.ccr |= OVERFLOW;
    cpu.step(&mut bus).unwrap();
    // Assert: A is unchanged and the test result controls N/Z.
    assert_eq!(cpu.registers.a, 0x80);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), NEGATIVE);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: EORA applies the final logical operation.
    cpu.registers.ccr |= OVERFLOW;
    cpu.step(&mut bus).unwrap();
    // Assert: The result and preserved flags match the logical-operation contract.
    assert_eq!(cpu.registers.a, 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & OVERFLOW, 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Arrange: Isolate ADC with A=0x7f and carry-in set at the signed boundary.
    let mut adc_bus = TestBus::default();
    adc_bus.set_reset_vector(0x2000);
    adc_bus.load(0x2000, &[0x89, 0x00]);
    let mut adc_cpu = Cpu6800::default();
    adc_cpu.reset(&mut adc_bus).unwrap();
    adc_cpu.registers.a = 0x7f;
    adc_cpu.registers.ccr |= CARRY;

    // Act: Execute ADCA in the independent fixture.
    adc_cpu.step(&mut adc_bus).unwrap();

    // Assert: Carry-in participates in the result and all arithmetic flags.
    assert_eq!(adc_cpu.registers.a, 0x80);
    assert_eq!(
        adc_cpu.registers.ccr & (NEGATIVE | HALF_CARRY | OVERFLOW),
        NEGATIVE | HALF_CARRY | OVERFLOW
    );
    assert_eq!(adc_cpu.registers.ccr & CARRY, 0);
}

// What: Walk the immediate B-register instruction family through its operations.
// Why: The shared A/B decoder must preserve compare/bit-test operands and update each flag group.
#[test]
fn immediate_b_instruction_family_updates_b_and_flags() {
    // Arrange: Encode loads, arithmetic, compare, logical, and carry-sensitive cases in sequence.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(
        0x2000,
        &[
            0xc6, 0xff, 0xc0, 0x01, 0xc1, 0xfe, 0xc2, 0xfd, 0xc4, 0x01, 0xc5, 0x00, 0xc8, 0x01,
            0xc9, 0x00, 0xca, 0x80, 0xc6, 0x80, 0xcb, 0x80,
        ],
    );
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Load B with 0xff.
    cpu.step(&mut bus).unwrap();
    // Assert: The load sets B and the negative flag.
    assert_eq!(cpu.registers.b, 0xff);
    assert_ne!(cpu.registers.ccr & NEGATIVE, 0);

    // Act: Subtract one from B.
    cpu.step(&mut bus).unwrap();
    // Assert: The result is 0xfe, and its flags feed the following comparison.
    assert_eq!(cpu.registers.b, 0xfe);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | CARRY), NEGATIVE);

    // Act: Compare B with 0xfe without storing the difference.
    cpu.step(&mut bus).unwrap();
    // Assert: B is preserved and equality is reported.
    assert_eq!(cpu.registers.b, 0xfe);
    assert_ne!(cpu.registers.ccr & ZERO, 0);

    // Act: Subtract 0xfd with the carry produced by CMPB.
    cpu.step(&mut bus).unwrap();
    // Assert: Carry-in changes the result to one.
    assert_eq!(cpu.registers.b, 1);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | ZERO | CARRY), 0);

    // Arrange: Seed carry/half-carry so the logical operations' preservation rules are visible.
    cpu.registers.ccr |= HALF_CARRY | CARRY;

    // Act: ANDB applies the immediate mask to B.
    cpu.registers.ccr |= OVERFLOW;
    cpu.step(&mut bus).unwrap();
    // Assert: B and unaffected flag bits remain as expected.
    assert_eq!(cpu.registers.b, 1);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | ZERO | OVERFLOW), 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: BITB tests against zero without replacing B.
    cpu.registers.ccr |= OVERFLOW;
    cpu.step(&mut bus).unwrap();
    // Assert: The test result sets Z while B remains unchanged.
    assert_eq!(cpu.registers.b, 1);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & OVERFLOW, 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: EORB toggles B's low bit; ADCB then consumes the still-set carry.
    cpu.registers.ccr |= OVERFLOW;
    cpu.step(&mut bus).unwrap();
    // Assert: EORB stores zero and preserves the carry for ADCB.
    assert_eq!(cpu.registers.b, 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & OVERFLOW, 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: ADCB adds the carry-in to the zero B value.
    cpu.step(&mut bus).unwrap();
    // Assert: Carry-in makes B one.
    assert_eq!(cpu.registers.b, 1);
    assert_eq!(cpu.registers.ccr & (ZERO | CARRY), 0);

    // Arrange: Restore carry and overflow before ORAB.
    cpu.registers.ccr |= HALF_CARRY | CARRY | OVERFLOW;
    // Act: ORAB sets B's high bit.
    cpu.step(&mut bus).unwrap();
    // Assert: B and logical-operation flags match the OR result.
    assert_eq!(cpu.registers.b, 0x81);
    assert_ne!(cpu.registers.ccr & NEGATIVE, 0);
    assert_eq!(cpu.registers.ccr & OVERFLOW, 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: LDAB replaces B with 0x80.
    cpu.step(&mut bus).unwrap();
    // Assert: The load establishes ADDB's second 0x80 operand.
    assert_eq!(cpu.registers.b, 0x80);

    // Act: ADDB crosses the signed and byte boundaries.
    cpu.step(&mut bus).unwrap();
    // Assert: The final sum is zero with carry and overflow set.
    assert_eq!(cpu.registers.b, 0);
    assert_eq!(
        cpu.registers.ccr & (ZERO | CARRY | OVERFLOW),
        ZERO | CARRY | OVERFLOW
    );
}

// What: Load positive-sign and zero values into B with unrelated flags pre-set.
// Why: Loads should recompute N/Z/V while preserving condition codes they do not define.
#[test]
fn immediate_b_load_updates_nzv_and_preserves_other_flags() {
    // Arrange: Seed stale N/Z/V and preservation flags before two immediate loads.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0xc6, 0x80, 0xc6, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.ccr |= HALF_CARRY | CARRY | OVERFLOW | ZERO;

    // Act: Load 0x80 and check its sign-related flags.
    cpu.step(&mut bus).unwrap();

    // Assert: N is set, Z/V are cleared, and unrelated flags survive.
    assert_eq!(cpu.registers.b, 0x80);
    assert_ne!(cpu.registers.ccr & NEGATIVE, 0);
    assert_eq!(cpu.registers.ccr & (ZERO | OVERFLOW), 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: Load zero to exercise the other side of the N/Z decision.
    cpu.step(&mut bus).unwrap();

    // Assert: Z is set while N/V remain clear and preserved flags are unchanged.
    assert_eq!(cpu.registers.b, 0);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );
}

// What: Repeat load-flag checks for accumulator A.
// Why: A and B use distinct opcode paths and both must preserve unrelated CCR bits.
#[test]
fn immediate_load_updates_nz_and_clears_overflow_without_changing_other_flags() {
    // Arrange: Seed stale flags before loading a negative byte and then zero.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0x80, 0x86, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.ccr |= HALF_CARRY | CARRY | OVERFLOW | ZERO;

    // Act: Load 0x80 into A.
    cpu.step(&mut bus).unwrap();

    // Assert: N replaces stale Z/V while H, C, and I are retained.
    assert_eq!(cpu.registers.a, 0x80);
    assert_ne!(cpu.registers.ccr & NEGATIVE, 0);
    assert_eq!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & OVERFLOW, 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );

    // Act: Load zero into A.
    cpu.step(&mut bus).unwrap();

    // Assert: Z is set and the unrelated flags remain untouched.
    assert_eq!(cpu.registers.a, 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );
}

// What: Store a zero-valued A to an extended address.
// Why: A successful store must write memory and derive N/Z/V from the stored value.
#[test]
fn extended_store_updates_nz_clears_overflow_and_preserves_other_flags() {
    // Arrange: A defaults to zero; seed stale result flags and unrelated flags.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0xb7, 0x30, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.ccr |= NEGATIVE | OVERFLOW | HALF_CARRY | CARRY;

    // Act: Execute STAA extended.
    cpu.step(&mut bus).unwrap();

    // Assert: Memory receives zero, Z is set, and only the defined flags change.
    assert_eq!(bus.memory[0x3000], 0);
    assert_ne!(cpu.registers.ccr & ZERO, 0);
    assert_eq!(cpu.registers.ccr & (NEGATIVE | OVERFLOW), 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );
}

// What: Store a negative nonzero A value over a stale zero flag.
// Why: The store path must set N and clear Z without relying on a prior load.
#[test]
fn extended_store_sets_negative_and_clears_zero_for_nonzero_value() {
    // Arrange: Seed A and CCR directly so the test isolates store flag behavior.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0xb7, 0x30, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.registers.a = 0x80;
    cpu.registers.ccr |= ZERO | OVERFLOW | HALF_CARRY | CARRY;

    // Act: Execute the extended store.
    cpu.step(&mut bus).unwrap();

    // Assert: Memory and N/Z/V reflect A, while unrelated condition bits persist.
    assert_eq!(bus.memory[0x3000], 0x80);
    assert_ne!(cpu.registers.ccr & NEGATIVE, 0);
    assert_eq!(cpu.registers.ccr & (ZERO | OVERFLOW), 0);
    assert_eq!(
        cpu.registers.ccr & (HALF_CARRY | CARRY | INTERRUPT_MASK),
        HALF_CARRY | CARRY | INTERRUPT_MASK
    );
}

// What: Attempt to execute an opcode outside the implemented instruction subset.
// Why: Unsupported bytes must fail explicitly rather than masquerading as successful no-ops.
#[test]
fn unsupported_opcode_reports_its_address_and_value() {
    // Arrange: Place an unsupported byte at the reset entry point.
    let mut bus = TestBus::default();
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x02]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Request one instruction step.
    assert_eq!(
        cpu.step(&mut bus),
        Err(CpuError::UnsupportedOpcode {
            address: 0x2000,
            opcode: 0x02,
        })
    );
    // Assert: The error identifies the opcode location/value and fetch progress remains visible.
    assert_eq!(cpu.registers.pc, 0x2001);
}

// What: Fail each byte read used to load the reset vector.
// Why: Reset must not partially replace registers if either vector byte is unavailable.
#[test]
fn reset_vector_read_failures_preserve_cpu_state_and_return_the_bus_error() {
    for failing_address in [RESET_VECTOR, RESET_VECTOR.wrapping_add(1)] {
        // Arrange: Fail one vector byte and seed CPU state that reset would otherwise clear.
        let mut bus = TestBus {
            fail_read_at: Some(failing_address),
            ..TestBus::default()
        };
        let mut cpu = Cpu6800 {
            registers: CpuRegisters {
                a: 0x5a,
                pc: 0x1234,
                ..CpuRegisters::default()
            },
        };
        let before = cpu.registers;

        // Act: Attempt reset with the selected vector read unavailable.
        assert_eq!(cpu.reset(&mut bus), Err(CpuError::Bus(failing_address)));
        // Assert: The bus error is returned and every register is preserved.
        assert_eq!(cpu.registers, before);
    }
}

// What: Fail an immediate operand read after the opcode itself was fetched.
// Why: Callers need predictable PC/IR state to diagnose a partially completed instruction.
#[test]
fn operand_read_failure_is_returned_after_opcode_fetch() {
    // Arrange: Permit the opcode read but reject the following operand address.
    let mut bus = TestBus {
        fail_read_at: Some(0x2001),
        ..TestBus::default()
    };
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0x42]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();

    // Act: Step the immediate load until the operand fetch fails.
    assert_eq!(cpu.step(&mut bus), Err(CpuError::Bus(0x2001)));
    // Assert: The opcode fetch is reflected, but the accumulator has not changed.
    assert_eq!(cpu.registers.pc, 0x2001);
    assert_eq!(cpu.registers.ir, 0x86);
    assert_eq!(cpu.registers.a, 0);
}

// What: Reject the memory write of an extended A store.
// Why: A failed bus write must be reported and must not be reported as a completed store.
#[test]
fn store_bus_failure_is_returned_without_reporting_success() {
    // Arrange: Load a value successfully, then configure the target address to reject writes.
    let mut bus = TestBus {
        fail_write_at: Some(0x3000),
        ..TestBus::default()
    };
    bus.set_reset_vector(0x2000);
    bus.load(0x2000, &[0x86, 0x5a, 0xb7, 0x30, 0x00]);
    let mut cpu = Cpu6800::default();
    cpu.reset(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();

    // Act: Execute STAA after the preceding load has completed.
    assert_eq!(cpu.step(&mut bus), Err(CpuError::Bus(0x3000)));
    // Assert: The error is surfaced and the target memory remains unchanged.
    assert_eq!(cpu.registers.pc, 0x2005);
    assert_eq!(bus.memory[0x3000], 0);
}
