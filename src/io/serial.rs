use crate::io::io::{inb, outb};
use crate::log;
static PORT: u16 = 0x3f8;

pub fn serial_init() {
    unsafe {
        outb(PORT + 1, 0x00); // Disable all interrupts
        outb(PORT + 3, 0x80); // Enable DLAB (set baud rate divisor)
        outb(PORT + 0, 0x03); // Set divisor to 3 (lo byte) 38400 baud
        outb(PORT + 1, 0x00); //                  (hi byte)
        outb(PORT + 3, 0x03); // 8 bits, no parity, one stop bit
        outb(PORT + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
        outb(PORT + 4, 0x0B); // IRQs enabled, RTS/DSR set
        outb(PORT + 4, 0x1E); // Set in loopback mode, test the serial chip
        outb(PORT + 0, 0xAE); // Test serial chip (send byte 0xAE and check if serial returns same byte)

        // serial check
        if inb(PORT + 0) != 0xAE {
            log!(
                crate::io::LogType::ERR,
                "Serial port is not working correctly"
            );

            return;
        }

        // set normal operation mode
        outb(PORT + 4, 0x0F);
    }
}

pub fn serial_received() -> bool {
    inb(PORT + 5) & 0x01 != 0
}

/// Takes one byte out of the receive FIFO.
///
/// The caller has to have checked `serial_received` first: reading an empty
/// FIFO returns whatever the register holds rather than blocking, and this
/// runs from the timer ISR where spinning is not an option.
pub fn read_serial() -> u8 {
    inb(PORT)
}

pub fn is_transmit_empty() -> bool {
    inb(PORT + 5) & 0x20 != 0
}

pub fn write_serial(char: u8) {
    while !is_transmit_empty() {}
    unsafe {
        outb(PORT, char);
    }
}

pub fn serial_write_str(text: &str) {
    for char in text.chars() {
        write_serial(char as u8);
    }
}
