// syscall 10 - close an open file descriptor

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    syscall::{Errno, SyscallResult},
};

pub fn close(stack: &FullInterruptStackFrame) -> SyscallResult {
    let fd = stack.rdi;

    if !scheduling::close_current_file_descriptor(fd) {
        return Some(Err(Errno::BadF));
    }

    Some(Ok(0))
}
