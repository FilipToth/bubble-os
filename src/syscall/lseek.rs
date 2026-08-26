// syscall 20 - move the offset of an open file descriptor

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    scheduling::process::{SEEK_CUR, SEEK_END, SEEK_SET},
    syscall::{Errno, SyscallResult},
};

pub fn lseek(stack: &FullInterruptStackFrame) -> SyscallResult {
    let file_descriptor = stack.rdi;

    // the offset is signed, SEEK_CUR and SEEK_END both take negative values
    let offset = stack.rsi as isize;
    let whence = stack.rdx;

    if whence != SEEK_SET && whence != SEEK_CUR && whence != SEEK_END {
        return Some(Err(Errno::Inval));
    }

    // a seek before the start of the file is the only in-range failure the
    // descriptor layer reports separately from a bad descriptor
    let Some(new_offset) = scheduling::seek_current_file_descriptor(file_descriptor, offset, whence)
    else {
        return Some(Err(Errno::BadF));
    };

    Some(Ok(new_offset))
}
