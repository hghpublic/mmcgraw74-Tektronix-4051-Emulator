//! Models the machine's PIA ports, data direction, and edge-latched control flags.
//!
//! Each port shares an address between its data and direction registers. Data
//! reads combine input and output bits and clear that port's latched flags;
//! control-line transitions latch flags according to the selected edge and mode.

const PIA_PORT_A_OFFSET: u16 = 0;
const PIA_CONTROL_A_OFFSET: u16 = 1;
const PIA_PORT_B_OFFSET: u16 = 2;
const PIA_CONTROL_B_OFFSET: u16 = 3;
const PIA_DATA_DIRECTION_SELECT_MASK: u8 = 0x04;
const PIA_CONTROL_WRITABLE_MASK: u8 = 0x3f;
const PIA_C1_FLAG_MASK: u8 = 0x80;
const PIA_C2_FLAG_MASK: u8 = 0x40;
const PIA_C2_OUTPUT_MODE_MASK: u8 = 0x20;
const PIA_C1_EDGE_SELECT_MASK: u8 = 0x02;
const PIA_C2_EDGE_SELECT_MASK: u8 = 0x08;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum PiaId {
    DisplayY = 0,
    DisplayX = 1,
    Tape = 2,
    Keyboard = 3,
    Gpib = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiaPort {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiaControlLine {
    Ca1,
    Ca2,
    Cb1,
    Cb2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PiaRegisters {
    control_a: u8,
    control_b: u8,
    output_a: u8,
    output_b: u8,
    input_a: u8,
    input_b: u8,
    direction_a: u8,
    direction_b: u8,
    ca1: bool,
    ca2: bool,
    cb1: bool,
    cb2: bool,
}

impl PiaRegisters {
    // Control bit 2 selects the data register; reading data also clears flags.
    pub(crate) fn read(&mut self, register: u16) -> u8 {
        match register {
            PIA_PORT_A_OFFSET if self.control_a & PIA_DATA_DIRECTION_SELECT_MASK != 0 => {
                self.control_a &= PIA_CONTROL_WRITABLE_MASK;
                (self.input_a & !self.direction_a) | (self.output_a & self.direction_a)
            }
            PIA_PORT_A_OFFSET => self.direction_a,
            PIA_CONTROL_A_OFFSET => self.control_a,
            PIA_PORT_B_OFFSET if self.control_b & PIA_DATA_DIRECTION_SELECT_MASK != 0 => {
                self.control_b &= PIA_CONTROL_WRITABLE_MASK;
                (self.input_b & !self.direction_b) | (self.output_b & self.direction_b)
            }
            PIA_PORT_B_OFFSET => self.direction_b,
            PIA_CONTROL_B_OFFSET => self.control_b,
            _ => 0,
        }
    }

    pub(crate) fn write(&mut self, register: u16, value: u8) {
        match register {
            PIA_PORT_A_OFFSET if self.control_a & PIA_DATA_DIRECTION_SELECT_MASK != 0 => {
                self.output_a = value
            }
            PIA_PORT_A_OFFSET => self.direction_a = value,
            PIA_CONTROL_A_OFFSET => self.control_a = value & PIA_CONTROL_WRITABLE_MASK,
            PIA_PORT_B_OFFSET if self.control_b & PIA_DATA_DIRECTION_SELECT_MASK != 0 => {
                self.output_b = value
            }
            PIA_PORT_B_OFFSET => self.direction_b = value,
            PIA_CONTROL_B_OFFSET => self.control_b = value & PIA_CONTROL_WRITABLE_MASK,
            _ => {}
        }
    }

    // C1 always accepts input edges, whereas C2 only latches them in input mode.
    pub(crate) fn signal_control_line(&mut self, line: PiaControlLine, level: bool) {
        let (previous, control, flag) = match line {
            PiaControlLine::Ca1 => (&mut self.ca1, &mut self.control_a, PIA_C1_FLAG_MASK),
            PiaControlLine::Ca2 => (&mut self.ca2, &mut self.control_a, PIA_C2_FLAG_MASK),
            PiaControlLine::Cb1 => (&mut self.cb1, &mut self.control_b, PIA_C1_FLAG_MASK),
            PiaControlLine::Cb2 => (&mut self.cb2, &mut self.control_b, PIA_C2_FLAG_MASK),
        };
        let input_mode = match line {
            PiaControlLine::Ca1 | PiaControlLine::Cb1 => true,
            PiaControlLine::Ca2 | PiaControlLine::Cb2 => *control & PIA_C2_OUTPUT_MODE_MASK == 0,
        };
        let edge_bit = match line {
            PiaControlLine::Ca1 | PiaControlLine::Cb1 => PIA_C1_EDGE_SELECT_MASK,
            PiaControlLine::Ca2 | PiaControlLine::Cb2 => PIA_C2_EDGE_SELECT_MASK,
        };
        if input_mode && *previous != level && (*control & edge_bit != 0) == level {
            *control |= flag;
        }
        *previous = level;
    }

    pub(crate) fn raise_ca1_interrupt(&mut self) {
        self.control_a |= PIA_C1_FLAG_MASK;
    }

    pub(crate) fn has_interrupt_flags(&self) -> bool {
        (self.control_a | self.control_b) & (PIA_C1_FLAG_MASK | PIA_C2_FLAG_MASK) != 0
    }

    pub(crate) fn set_input(&mut self, port: PiaPort, value: u8) {
        match port {
            PiaPort::A => self.input_a = value,
            PiaPort::B => self.input_b = value,
        }
    }

    pub(crate) fn set_input_bit(&mut self, port: PiaPort, mask: u8, high: bool) {
        let input = match port {
            PiaPort::A => &mut self.input_a,
            PiaPort::B => &mut self.input_b,
        };
        if high {
            *input |= mask;
        } else {
            *input &= !mask;
        }
    }

    pub(crate) fn is_data_direction_register_selected(&self, port: PiaPort) -> bool {
        let control = match port {
            PiaPort::A => self.control_a,
            PiaPort::B => self.control_b,
        };
        control & PIA_DATA_DIRECTION_SELECT_MASK != 0
    }

    pub(crate) fn output_bit_is_high(&self, port: PiaPort, mask: u8) -> bool {
        let output = match port {
            PiaPort::A => self.output_a,
            PiaPort::B => self.output_b,
        };
        output & mask != 0
    }

    pub(crate) fn output(&self, port: PiaPort) -> u8 {
        match port {
            PiaPort::A => self.output_a,
            PiaPort::B => self.output_b,
        }
    }
}
