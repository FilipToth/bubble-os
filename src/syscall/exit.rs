// syscall 1 - exit the current process

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling, syscall::SyscallResult};

/// Statuses are truncated to this many bits, as `WEXITSTATUS` does.
const EXIT_STATUS_MASK: usize = 0xFF;

pub fn exit(stack: &FullInterruptStackFrame) -> SyscallResult {
    // wait_for_process hands the status back over an ABI where negative
    // values mean errors, so a process must not be able to exit with one.
    // Masking to a byte is what POSIX exposes through WEXITSTATUS anyway,
    // and it still fits the 128 + vector statuses the fault handler uses
    let status = stack.rdi & EXIT_STATUS_MASK;

    scheduling::exit_current(status);
    scheduling::schedule(None);

    None
}
