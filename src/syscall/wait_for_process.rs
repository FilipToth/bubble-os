// syscall 6 - wait for a process to exit

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

pub fn wait_for_process(stack: &mut FullInterruptStackFrame) -> Option<usize> {
    let pid = stack.rdi;

    // the child can exit between the parent's execute and this call, one
    // timer tick is enough. Its status is already recorded, so there is
    // nothing left to wait for
    if let Some(status) = scheduling::take_exit_status(pid) {
        return Some(status);
    }

    // the scheduler resumes this process from inside next_process and
    // overwrites rax with the child status. Give rax a defined value first,
    // otherwise a wait for a pid that never existed resumes with the
    // syscall number still sitting in it
    stack.rax = 0;

    scheduling::current_wait_for_process(pid);
    scheduling::schedule(Some(stack));

    None
}
