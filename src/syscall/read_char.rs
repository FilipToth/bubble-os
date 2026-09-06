// syscall 24 - read one raw byte from the console

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    io::console,
    scheduling,
    syscall::{read::SYSCALL_INSTRUCTION_LEN, SyscallResult},
};

/// Reads a single byte with no echo and no line editing.
///
/// This is the mechanism `read` used to have, kept because `edit` needs raw
/// keystrokes and cannot wait for a line to be finished. Which of the two
/// calls a process makes is what decides whether its input is echoed, so
/// there is no mode to set anywhere.
pub fn read_char(stack: &mut FullInterruptStackFrame) -> SyscallResult {
    let Some(byte) = console::take_byte() else {
        scheduling::block_current();

        // re-run this syscall on the way back rather than have the waker
        // write an answer into rax, the same as the canonical read
        stack.rip -= SYSCALL_INSTRUCTION_LEN;
        scheduling::schedule(Some(&*stack));

        return None;
    };

    Some(Ok(byte as usize))
}
