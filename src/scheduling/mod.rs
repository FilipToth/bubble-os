use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{sync::Arc, vec::Vec};
use process::{Process, ProcessEntry};
use spin::Mutex;

use crate::{
    arch::x86_64::{gdt::GDT, registers::FullInterruptStackFrame},
    elf,
    fs::fs::Directory,
    mem::GLOBAL_MEMORY_CONTROLLER,
    print, with_root_dir,
};

pub mod process;

pub static SCHEDULING_ENABLED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
pub static PID_COUNTER: AtomicUsize = AtomicUsize::new(0);

unsafe fn jump(context: &FullInterruptStackFrame) {
    let ctx_addr = context as *const FullInterruptStackFrame as usize;

    core::arch::asm!(
        "cli",

        "push {ss}",
        "push {rsp}",
        "push {rflags}",
        "push {cs}",
        "push {rip}",
        "push [{ctx} + 0x00]", // r8
        "push [{ctx} + 0x08]", // r9
        "push [{ctx} + 0x10]", // r10
        "push [{ctx} + 0x18]", // r11
        "push [{ctx} + 0x20]", // r12
        "push [{ctx} + 0x28]", // r13
        "push [{ctx} + 0x30]", // r14
        "push [{ctx} + 0x38]", // r15
        "push [{ctx} + 0x40]", // rbp
        "push [{ctx} + 0x48]", // rdi
        "push [{ctx} + 0x50]", // rsi
        "push [{ctx} + 0x58]", // rdx
        "push [{ctx} + 0x60]", // rcx
        "push [{ctx} + 0x68]", // rbx
        "push [{ctx} + 0x70]", // rax

        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",

        "iretq",

        ss = in(reg) context.ss,
        rsp = in(reg) context.rsp,
        rflags = in(reg) context.rflags,
        cs = in(reg) context.cs,
        rip = in(reg) context.rip,
        ctx = in(reg) ctx_addr,
        options(noreturn)
    );
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

    unsafe { jump(&process_to_jump.context) };
}

pub fn deploy(entry: ProcessEntry, fork_current: bool) -> usize {
    let pid = PID_COUNTER.load(Ordering::SeqCst);
    PID_COUNTER.store(pid + 1, Ordering::SeqCst);

    loop {}

    let mut processes = PROCESSES.lock();
    let cwd = if fork_current && processes.len() != 0 {
        // basically fork the cwd from calling process
        let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
        let current = &processes[current_index];
        current.curr_working_dir.clone()
    } else {
        // root directory
        with_root_dir!(root, { root })
    };

    let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
    let mc = mc.as_mut().unwrap();

    // allocate process stack
    let stack = match mc.alloc_stack(10, true) {
        Some(s) => s,
        None => unreachable!(),
    };

    let mut process = Process::from(entry, pid, cwd, stack);
    let cs = GDT.1.user_code.0;
    let ss = GDT.1.user_data.0;

    process.context.cs = cs as usize;
    process.context.ss = ss as usize;
    process.context.rflags = 0x202;

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

pub fn get_current_cwd() -> Arc<dyn Directory> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        with_root_dir!(root, { root })
    } else {
        let current_process = &mut processes[current_index];
        let cwd = &current_process.curr_working_dir;

        print!(
            "Loading cwd {}, for index: {}, pid: {}\n",
            cwd.name(),
            current_index,
            current_process.pid
        );

        cwd.clone()
    }
}

pub fn change_cwd(cwd: Arc<dyn Directory + Send + Sync>) {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current_process = &mut processes[current_index];
    print!(
        "Saving cwd: {}, to index: {}, pid: {}\n",
        cwd.name(),
        current_index,
        current_process.pid
    );

    current_process.curr_working_dir = cwd.clone();
}

pub fn enable() {
    print!("[ SCHED ] Enabled Scheduling!\n");
    SCHEDULING_ENABLED.store(true, Ordering::SeqCst);
}
