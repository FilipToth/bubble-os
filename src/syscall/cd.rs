use alloc::string::ToString;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, fs::GLOBAL_FILESYSTEM, print, scheduling
};

pub fn cd(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let filename =
        core::str::from_utf8(slice).unwrap_or("Invalid ELF filename string for cd syscall\n");

    let mut fs = GLOBAL_FILESYSTEM.lock();
    let fs = fs.as_mut().unwrap();

    let cwd = scheduling::get_current_cwd();
    let new_path = fs.combine_path(cwd, filename.to_string());
    scheduling::change_cwd(new_path);

    None
}
