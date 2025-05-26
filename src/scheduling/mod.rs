use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::vec::Vec;
use process::{Process, ProcessEntry};
use spin::Mutex;

use crate::{arch::x86_64::registers::FullInterruptStackFrame, print};

pub mod process;

pub static SCHEDULING_ENABLED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
pub static PID_COUNTER: AtomicUsize = AtomicUsize::new(0);

unsafe fn jump(context: FullInterruptStackFrame) {
    core::arch::asm!("cli");

    if context.rsp == 0 {
        // uninitialized process,
        // empty jump no context switch
        core::arch::asm!("sti", "jmp {rip}", rip = in(reg) context.rip);
    }

    // our entire context switch is very ugly, but it's ok for now...
    // first we load CPU flags because that doesn't affect registers for
    // the actual general-purpose registers switch. Then, we manually
    // push the context's saved instruction pointer onto the ELF stack,
    // this is done because Rust's inline asm uses GP registers to pass
    // in values, and when we do our actual jump (using ret), we have
    // already loaded the context's registers, and thus shouldn't use
    // them. We also push the stack pointer onto the (now kernel) stack.
    // Then we push GP-registers onto the stack using inline asm, we can't
    // move them directly since Rust can't guarantee correct register
    // alignment here, so this is a bit of a workaround... Then we pop
    // GP registers off the stack into the correct registers (previously,
    // when we pushed them, Rust would use whatever registers it would
    // feel like), then we pop the ELF stack pointer into rsp, then we
    // simply enable interrupts and return, which just pops the rip off
    // the stack and jumps to it.

    core::arch::asm!(
        "push {rflags}",
        "popfq",

        rflags = in(reg) context.rflags
    );

    // manually push context rip to ELF stack
    let rsp_bottom = context.rsp - 8;
    let rsp_ptr = rsp_bottom as *mut usize;
    *rsp_ptr = context.rip;

    core::arch::asm!(
        "push {rsp}",
        rsp = in(reg) context.rsp
    );

    core::arch::asm!(
        "push {rax}",
        "push {rbx}",
        "push {rcx}",
        "push {rdx}",
        "push {rsi}",
        "push {rdi}",
        "push {rbp}",
        "push {r15}",
        "push {r14}",
        "push {r13}",
        "push {r12}",
        "push {r11}",
        "push {r10}",
        "push {r9}",
        "push {r8}",

        rax = in(reg) context.rax,
        rbx = in(reg) context.rbx,
        rcx = in(reg) context.rcx,
        rdx = in(reg) context.rdx,
        rsi = in(reg) context.rsi,
        rdi = in(reg) context.rdi,
        rbp = in(reg) context.rbp,
        r15 = in(reg) context.r15,
        r14 = in(reg) context.r14,
        r13 = in(reg) context.r13,
        r12 = in(reg) context.r12,
        r11 = in(reg) context.r11,
        r10 = in(reg) context.r10,
        r9 = in(reg) context.r9,
        r8 = in(reg) context.r8,
    );

    core::arch::asm!(
        "pop r8", "pop r9", "pop r10", "pop r11", "pop r12", "pop r13", "pop r14", "pop r15",
        "pop rbp", "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rbx", "pop rax",
    );

    core::arch::asm!("pop rsp", "sub rsp, 0x08");
    core::arch::asm!("sti", "ret", options(noreturn));
}

fn next_process(interrupt_stack: &FullInterruptStackFrame) -> Option<Process> {
    let mut current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let mut processes = PROCESSES.lock();
    let processes_len = processes.len();

    if processes_len == 0 {
        return None;
    }

    {
        let current = &mut processes[current_index];

        // Avoid saving kernel
        if !current.pre_schedule && interrupt_stack.rip > 0x10F000 {
            // save current context
            // print!("Saved context, rip => 0x{:x}\n", interrupt_stack.rip);
            current.context = interrupt_stack.clone();
        }
    }

    let mut passes = 0;
    loop {
        current_index = if current_index + 1 >= processes_len {
            0
        } else {
            current_index + 1
        };

        let new_current = &mut processes[current_index];
        if !new_current.blocking {
            new_current.pre_schedule = false;
            CURRENT_INDEX.store(current_index, Ordering::SeqCst);

            return Some(new_current.clone());
        }

        passes += 1;
        if passes >= processes_len {
            return None;
        }
    }
}

pub fn schedule(interrupt_stack: &FullInterruptStackFrame) {
    let process_to_jump = match next_process(interrupt_stack) {
        Some(p) => p,
        None => {
            unsafe { core::arch::asm!("sti") };
            loop {}
        }
    };

    let index = CURRENT_INDEX.load(Ordering::SeqCst);
    /*
    print!(
        "[ SCHED ] Jumping to process context ({}), rip: 0x{:x}, rsp: 0x{:x}, rax: 0x{:x}, rbx: 0x{:x}, r8: 0x{:x}\n",
        index,
        process_to_jump.context.rip,
        process_to_jump.context.rsp,
        process_to_jump.context.rax,
        process_to_jump.context.rbx,
        process_to_jump.context.r8
    );
    */

    unsafe { jump(process_to_jump.context) };
}

pub fn deploy(entry: ProcessEntry) {
    let pid = PID_COUNTER.load(Ordering::SeqCst);
    PID_COUNTER.store(pid + 1, Ordering::SeqCst);

    let process = Process::from(entry, pid);
    let mut processes = PROCESSES.lock();
    processes.push(process);
}

pub fn block_current() {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current = &mut processes[current_index];
    print!(
        "Blocking current => {}, rip => 0x{:x}\n",
        current_index, current.context.rip
    );

    current.blocking = true;
}

pub fn process_input(input: char) {
    let mut processes = PROCESSES.lock();
    for process in processes.iter_mut() {
        if !process.blocking {
            continue;
        }

        // process is awaiting input
        process.context.rax = input as usize;
        process.blocking = false;
    }
}

pub fn enable() {
    print!("[ SCHED ] Enabled Scheduling!\n");
    SCHEDULING_ENABLED.store(true, Ordering::SeqCst);
}
