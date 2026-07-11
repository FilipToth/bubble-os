// syscall 2 - write bytes to a file descriptor

use alloc::format;

use crate::log;
use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    print, scheduling,
    scheduling::process::{FileDescriptor, Process},
};

pub fn write(stack: &FullInterruptStackFrame) -> Option<usize> {
    let file_descriptor = stack.rdi;
    let buffer_addr = stack.rsi;
    let buffer_size = stack.rdx;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(0);
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        return Some(0);
    };

    let string = match core::str::from_utf8(&buffer) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!(
                "Invalid string for write syscall, rdi: 0x{:X}, rsi: 0x{:X}, rdx: 0x{:X}\n",
                file_descriptor, buffer_addr, buffer_size
            );

            log!(crate::io::LogType::ERR, "{}\n{:?}", msg, e);
            return Some(0);
        }
    };

    match scheduling::get_current_file_descriptor(file_descriptor) {
        Some(FileDescriptor::Stdout) | Some(FileDescriptor::Stderr) => {
            print!("{}", string);
            Some(buffer.len())
        }
        Some(FileDescriptor::File(_)) => Some(0),
        _ => Some(0),
    }
}
