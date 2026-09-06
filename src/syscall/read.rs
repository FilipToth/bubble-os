// syscall 3 - read bytes from stdin or a file descriptor

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    io::console,
    scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

/// Length of the `int 0x80` encoding, `CD 80`.
///
/// A read that has to wait rewinds `rip` past it so the syscall re-executes
/// when the process is scheduled again. That is how a blocking read answers
/// with a byte count: nothing can write the count into `rax` on the process'
/// behalf, because only the drain in here knows what it will be.
pub const SYSCALL_INSTRUCTION_LEN: usize = 2;

pub fn read(stack: &mut FullInterruptStackFrame) -> SyscallResult {
    let file_descriptor = stack.rdi;
    let buffer_addr = stack.rsi;
    let buffer_size = stack.rdx;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(Err(Errno::Srch));
    };

    if file_descriptor >= 3 {
        if !Process::can_process_pointer(&page_table, buffer_addr, buffer_size, true) {
            return Some(Err(Errno::Fault));
        }

        let Some(bytes) = scheduling::read_current_file_descriptor(file_descriptor, buffer_size)
        else {
            return Some(Err(Errno::BadF));
        };

        if Process::copy_to_user(&page_table, buffer_addr, &bytes).is_none() {
            return Some(Err(Errno::Fault));
        }

        // a zero length read is end of file, which is a success. Errors are
        // negative, so the two are no longer the same answer
        return Some(Ok(bytes.len()));
    }

    if file_descriptor != 0 {
        // stdout and stderr are write only
        return Some(Err(Errno::BadF));
    }

    if buffer_size == 0 {
        return Some(Ok(0));
    }

    if !Process::can_process_pointer(&page_table, buffer_addr, buffer_size, true) {
        return Some(Err(Errno::Fault));
    }

    // echoes and edits whatever has arrived since the last call, which is
    // also what makes typing visible
    if !console::poll_line() {
        scheduling::block_current();
        stack.rip -= SYSCALL_INSTRUCTION_LEN;

        // yield back to the scheduler instead of the caller
        scheduling::schedule(Some(&*stack));

        return None;
    }

    // capped at a line rather than at what the caller asked for, there is
    // never more than one line waiting
    let capacity = core::cmp::min(buffer_size, console::LINE_MAX);
    let mut buffer = alloc::vec![0u8; capacity];

    // zero means the line ended in end of file, which newlib reads as EOF
    let count = console::take_line(&mut buffer);
    if count == 0 {
        return Some(Ok(0));
    }

    if Process::copy_to_user(&page_table, buffer_addr, &buffer[..count]).is_none() {
        return Some(Err(Errno::Fault));
    }

    Some(Ok(count))
}
