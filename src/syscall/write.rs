use alloc::format;

use crate::{arch::x86_64::registers::FullInterruptStackFrame, print};

pub fn write(stack: &FullInterruptStackFrame) -> Option<usize> {
    let file_descriptor = stack.rdi;
    let buffer_addr = stack.rsi;
    let buffer_size = stack.rdx;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let string = match core::str::from_utf8(slice) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Invalid string for write syscall, rdi: 0x{:X}, rsi: 0x{:X}, rdx: 0x{:X}\n", file_descriptor, buffer_addr, buffer_size);
            print!("{}\n{:?}", msg, e);
            return Some(0);
        }
    };

    match file_descriptor {
        1 => {
            // stdout write
            print!("{}", string);
        }
        _ => {}
    }

    Some(0)
}
