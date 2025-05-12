use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::vec::Vec;
use process::{Process, ProcessEntry};
use spin::Mutex;
use x86_64::structures::idt::InterruptStackFrame;

use crate::print;

pub mod process;

pub static SCHEDULING_ENABLED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
pub static PID_COUNTER: AtomicUsize = AtomicUsize::new(0);

unsafe fn jump(proc: Process) {
    let rip = proc.rip;
    let rsp = proc.rsp;
    let rflags = proc.rflags;

    core::arch::asm!(
        "cli",

        "mov rsp, {rsp}",

        "push {rflags}",
        "popfq",

        "sti",
        "jmp {rip}",

        rsp = in(reg) rsp,
        rflags = in(reg) rflags,
        rip = in(reg) rip,

        options(noreturn)
    );
}

pub fn deploy(entry: ProcessEntry) {
    let pid = PID_COUNTER.load(Ordering::SeqCst);
    PID_COUNTER.store(pid + 1, Ordering::SeqCst);

    let process = Process::from(entry, pid);
    let mut processes = PROCESSES.lock();
    processes.push(process);
}

pub fn schedule(interrupt_stack: &InterruptStackFrame) {
    let process_to_jump: Option<Process>;

    // need a new scope to drop mutex lock
    {
        let mut processes = PROCESSES.lock();
        let processes_len = processes.len();
        if processes_len == 0 {
            return;
        }

        let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
        let current = &mut processes[current_index];

        // switch next process, round robin
        if current_index + 1 >= processes_len {
            CURRENT_INDEX.store(0, Ordering::SeqCst);
        } else {
            CURRENT_INDEX.store(current_index + 1, Ordering::SeqCst);
        }

        if current.pre_schedule {
            // first schedule call on current process
            current.pre_schedule = false;

            // do first jump, do no change rip,
            // since this isn't a context switch,
            // rip should contain entry address
            process_to_jump = Some(current.clone());
        } else {
            // regular scheduling context switch
            let rip = interrupt_stack.instruction_pointer.as_u64() as usize;
            let rsp = interrupt_stack.stack_pointer.as_u64() as usize;
            let rflags = interrupt_stack.cpu_flags as usize;

            current.rip = rip;
            current.rsp = rsp;
            current.rflags = rflags;

            process_to_jump = Some(current.clone());
        }
    }

    if let Some(process_to_jump) = process_to_jump {
        unsafe { jump(process_to_jump) };
    }
}

pub fn enable() {
    print!("[ SCHED ] Enabled Scheduling!\n");
    SCHEDULING_ENABLED.store(true, Ordering::SeqCst);
}
