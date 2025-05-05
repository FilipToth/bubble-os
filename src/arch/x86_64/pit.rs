use crate::{io::io, print};

pub const PIT_FREQUENCY: usize = 1193182;
pub const PIT_HZ: usize = 100;
pub const PIT_CMD_PORT: u16 = 0x43;
pub const PIT_CH0_PORT: u16 = 0x40;

fn unmask_irq(irq: u8) {
    let mut mask = io::inb(0x21);
    mask &= !(1 << irq);

    unsafe {
        io::outb(0x21, mask);
    }
}

pub fn end_of_interrupt(irq: u8) {
    unsafe {
        if irq >= 8 {
            io::outb(0xA0, 0x20);
        }

        io::outb(0x20, 0x20);
    }
}

pub fn init_pit() {
    let divisor = PIT_FREQUENCY / PIT_HZ;
    print!("[ OK ] Initialized PIT, divisor: {}\n", divisor);

    unsafe {
        io::outb(PIT_CMD_PORT, 0x36);
        io::outb(PIT_CH0_PORT, (divisor & 0xFF) as u8);
        io::outb(PIT_CH0_PORT, (divisor >> 8) as u8);
    }

    unmask_irq(0);
}
