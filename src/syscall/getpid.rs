// syscall 23 - the pid of the calling process

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    syscall::{Errno, SyscallResult},
};

pub fn getpid(_stack: &FullInterruptStackFrame) -> SyscallResult {
    // pids start at 1, so a caller can never mistake one for an error
    let Some(pid) = scheduling::current_pid() else {
        return Some(Err(Errno::Srch));
    };

    Some(Ok(pid))
}
