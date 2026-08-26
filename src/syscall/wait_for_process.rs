// syscall 6 - wait for a process to exit

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    scheduling,
    syscall::{self, Errno, SyscallResult},
};

pub fn wait_for_process(stack: &mut FullInterruptStackFrame) -> SyscallResult {
    let pid = stack.rdi;

    // the child can exit between the parent's execute and this call, one
    // timer tick is enough. Its status is already recorded, so there is
    // nothing left to wait for
    if let Some(status) = scheduling::take_exit_status(pid) {
        return Some(Ok(status));
    }

    // pid 0 is never handed out, so it cannot be waited on
    if !scheduling::process_exists(pid) {
        return Some(Err(Errno::Srch));
    }

    // the scheduler resumes this process from inside next_process and
    // overwrites rax with the child status. Give rax a defined value first,
    // in case the process disappears without leaving a record behind
    stack.rax = syscall::encode(Err(Errno::Srch));

    scheduling::current_wait_for_process(pid);
    scheduling::schedule(Some(stack));

    None
}
