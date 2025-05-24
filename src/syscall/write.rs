use crate::{arch::x86_64::registers::FullInterruptStackFrame, print};

pub fn write(stack: &FullInterruptStackFrame) {
    let file_descriptor = stack.rdi;
    let buffer_addr = stack.rsi;
    let buffer_size = stack.r11;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let string = core::str::from_utf8(slice).unwrap_or("Invalid string for write syscall");

    match file_descriptor {
        1 => {
            // stdout write
            print!("{}", string);
        }
        _ => {}
    }
}
