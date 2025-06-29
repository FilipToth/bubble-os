use alloc::format;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, fs::combine_path, print, scheduling,
};

pub fn cd(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let filename = match core::str::from_utf8(slice) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for change directory syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            print!("{}\n{:?}\n", msg, e);
            return Some(0);
        }
    };

    let filename = filename.trim();

    let cwd = scheduling::get_current_cwd();
    let new_path = combine_path(&cwd, filename);
    scheduling::change_cwd(new_path);

    None
}
