use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use process::{Process, ProcessEntry};
use spin::Mutex;

use crate::{arch::x86_64::registers::FullInterruptStackFrame, elf, fs::{fat_fs::FATFileSystem, fs::{self, DirectoryKind, FileSystem}, GLOBAL_FILESYSTEM}, print, with_fs};

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

fn next_process(interrupt_stack: Option<&FullInterruptStackFrame>) -> Option<Process> {
    let mut current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let mut processes = PROCESSES.lock();
    let processes_len = processes.len();

    if processes_len == 0 {
        return None;
    }

    if let Some(interrupt_stack) = interrupt_stack {
        match processes.get_mut(current_index) {
            Some(current) => {
                // the bug happens when accessing anything from current after we call the exit syscall

                // Avoid saving kernel
                let _m = current.pid + 1;
                let is_not_presched = !current.pre_schedule;
                let rip = interrupt_stack.rip;

                if is_not_presched && rip > 0x1FFFFF {
                    // save current context
                    current.context = interrupt_stack.clone();
                }
            }
            None => {}
        };
    }

    let mut passes = 0;
    loop {
        current_index = if current_index + 1 >= processes_len {
            0
        } else {
            current_index + 1
        };

        let (blocking, awaiting_process) = {
            let process = &mut processes[current_index];
            (process.blocking, process.awaiting_process)
        };

        let mut new_current_ready = !blocking;
        if let Some(subprocess_pid) = awaiting_process {
            let process_found = processes.iter().any(|p| p.pid == subprocess_pid);
            new_current_ready = !process_found;
        }

        if new_current_ready {
            CURRENT_INDEX.store(current_index, Ordering::SeqCst);

            let new_current = &mut processes[current_index];
            new_current.pre_schedule = false;
            new_current.awaiting_process = None;

            return Some(new_current.clone());
        }

        passes += 1;
        if passes >= processes_len {
            return None;
        }
    }
}

pub fn schedule(interrupt_stack: Option<&FullInterruptStackFrame>) {
    let process_to_jump = match next_process(interrupt_stack) {
        Some(p) => p,
        None => {
            unsafe { core::arch::asm!("sti") };
            loop {}
        }
    };

    unsafe { jump(process_to_jump.context) };
}

pub fn deploy(entry: ProcessEntry, fork_current: bool) -> usize {
    let pid = PID_COUNTER.load(Ordering::SeqCst);
    PID_COUNTER.store(pid + 1, Ordering::SeqCst);

    let mut processes = PROCESSES.lock();
    let cwd = if fork_current && processes.len() != 0 {
        // basically fork the cwd from calling process
        let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
        let current = &processes[current_index];
        current.curr_working_dir.clone()
    } else {
        with_fs!(FATFileSystem, fs, {
            let root = fs.root();
            DirectoryKind::FATDirectory(root)
        })
    };

    let process = Process::from(entry, pid, cwd);
    processes.push(process);
    pid
}

pub fn block_current() {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current = &mut processes[current_index];
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

pub fn current_wait_for_process(subprocess: usize) {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    // TODO: Check if subprocess PID exists
    let current = &mut processes[current_index];
    current.awaiting_process = Some(subprocess);
}

pub fn exit_current() {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let removed = processes.remove(current_index);

    elf::unmap(&removed.start_region);

    // adjust current process index
    let new_index = if current_index != 0 {
        current_index - 1
    } else {
        0
    };

    CURRENT_INDEX.store(new_index, Ordering::SeqCst);
}

pub fn get_current_cwd() -> DirectoryKind {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        with_fs!(FATFileSystem, fs, {
            let root = fs.root();
            DirectoryKind::FATDirectory(root)
        })
    } else {
        let current_process = &mut processes[current_index];
        let cwd = &current_process.curr_working_dir;

        print!(
            "Loading cwd {}, for index: {}, pid: {}\n",
            cwd.name(), current_index, current_process.pid
        );

        cwd.clone()
    }
}

pub fn change_cwd(cwd: DirectoryKind) {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current_process = &mut processes[current_index];
    print!(
        "Saving cwd: {}, to index: {}, pid: {}\n",
        cwd.name(), current_index, current_process.pid
    );

    current_process.curr_working_dir = cwd.clone();
}

pub fn enable() {
    print!("[ SCHED ] Enabled Scheduling!\n");
    SCHEDULING_ENABLED.store(true, Ordering::SeqCst);
}
