use alloc::{format, string::ToString};

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, fs::GLOBAL_FILESYSTEM, print, scheduling
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

    let mut fs = GLOBAL_FILESYSTEM.lock();
    let fs = fs.as_mut().unwrap();

    let cwd = scheduling::get_current_cwd();
    let new_path = fs.combine_path(cwd, filename.to_string());
    scheduling::change_cwd(new_path);

    None
}
