use core::sync::atomic::Ordering;

use crate::{
    arch, print,
    scheduling::{self, SCHEDULING_ENABLED},
};

#[repr(C)]
#[derive(Clone)]
pub struct FullInterruptStackFrame {
    pub r8: usize,
    pub r9: usize,
    pub r10: usize,
    pub r11: usize,
    pub r12: usize,
    pub r13: usize,
    pub r14: usize,
    pub r15: usize,
    pub rbp: usize,
    pub rdi: usize,
    pub rsi: usize,
    pub rdx: usize,
    pub rcx: usize,
    pub rbx: usize,
    pub rax: usize,

    // automatically pushed by CPU
    pub rip: usize,
    pub cs: usize,
    pub rflags: usize,
    pub rsp: usize,
    pub ss: usize,
}

impl FullInterruptStackFrame {
    pub fn empty() -> Self {
        Self {
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        }
    }
}

#[naked]
pub unsafe extern "x86-interrupt" fn timer_trampoline() {
    core::arch::asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "mov rdi, rsp",
        "call timer_isr_rust",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "iretq",
        options(noreturn)
    );
}

#[no_mangle]
pub extern "C" fn timer_isr_rust(stack: *mut FullInterruptStackFrame) {
    let sched_enabled = SCHEDULING_ENABLED.load(Ordering::SeqCst);
    arch::x86_64::pit::end_of_interrupt(0);

    let stack = unsafe { &mut *stack };
    if sched_enabled {
        scheduling::schedule(&stack);
    }
}
