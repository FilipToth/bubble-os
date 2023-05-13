use crate::{io::{inb, outb}, print};

static port: u16 = 0x3f8;

pub fn serial_init() {

    unsafe {
        outb(port + 1, 0x00); // Disable all interrupts
        outb(port + 3, 0x80); // Enable DLAB (set baud rate divisor)
        outb(port + 0, 0x03); // Set divisor to 3 (lo byte) 38400 baud
        outb(port + 1, 0x00); //                  (hi byte)
        outb(port + 3, 0x03); // 8 bits, no parity, one stop bit
        outb(port + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
        outb(port + 4, 0x0B); // IRQs enabled, RTS/DSR set
        outb(port + 4, 0x1E); // Set in loopback mode, test the serial chip
        outb(port + 0, 0xAE); // Test serial chip (send byte 0xAE and check if serial returns same byte)

        // serial check
        if inb(port + 0) != 0xAE {
            print!("Serial port is not working correctly\n");
            return;
        }

        // set normal operation mode
        outb(port + 4, 0x0F);
    }
}

pub fn is_transmit_empty() -> bool {
    inb(port + 5) & 0x20 != 0
}

pub fn write_serial(char: u8) {
    while !is_transmit_empty() {}
    unsafe {
        outb(port, char);
    }
}

pub fn serial_write_str(text: &str) {
    for char in text.chars() {
        write_serial(char as u8);
    }
}

// for more info and serial protocol documentation:
// https://wiki.osdev.org/Serial_Ports
