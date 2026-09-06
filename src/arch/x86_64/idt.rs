use core::ops::IndexMut;

use x86_64::{
    registers::control::Cr2,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
    PrivilegeLevel, VirtAddr,
};

use crate::log;
use crate::{
    arch::x86_64::{
        gdt::{DOUBLE_FAULT_STACK_INDEX, PIT_STACK_INDEX, SYSCALL_STACK_INDEX},
        timer_isr::timer_trampoline,
    },
    interrupt_trampoline,
    io::io,
    print, scheduling, syscall,
};

use super::registers::FullInterruptStackFrame;

pub const IRQ0: usize = 0x20;

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

extern "x86-interrupt" fn breakpoint_isr(_stack: InterruptStackFrame) {
    log!(
        crate::io::LogType::EXCEPTION,
        "Breakpoint interrupt called!"
    );

    loop {}
}

extern "x86-interrupt" fn double_fault_isr(stack: InterruptStackFrame, err_code: u64) -> ! {
    log!(
        crate::io::LogType::EXCEPTION,
        "Double fault, err_code: 0x{:x}",
        err_code
    );

    log!(crate::io::LogType::ERR, "Dumping stack frame\n{:#?}", stack);
    loop {}
}

/// The exit status of a killed process is this plus the exception vector, so
/// a program that died to a fault can be told apart from one that chose its
/// own status.
const FAULT_EXIT_STATUS_BASE: usize = 128;

/// Handles a CPU exception that could have been raised by a user program.
///
/// A ring 3 fault is fatal to the faulting program, but it must not take the
/// kernel down with it, so the offending process is killed and the scheduler
/// moves on to the next one. A ring 0 fault is a kernel bug, there is nothing
/// sane to return to, so we dump the frame and halt.
///
/// ## Arguments
///
/// - `stack` the exception stack frame pushed by the CPU
/// - `vector` the exception vector, used to build the exit status
/// - `name` the human readable name of the exception, used when logging
fn handle_fault(stack: &InterruptStackFrame, vector: usize, name: &str) {
    // the low two bits of the saved cs hold the privilege level the
    // exception was raised at
    let from_userspace = stack.code_segment & 3 != 0;
    if !from_userspace {
        log!(crate::io::LogType::ERR, "{} in kernel mode, halting", name);
        log!(crate::io::LogType::ERR, "Dumping stack frame\n{:#?}", stack);
        loop {}
    }

    let status = FAULT_EXIT_STATUS_BASE + vector;
    match scheduling::current_pid() {
        Some(pid) => log!(
            crate::io::LogType::EXCEPTION,
            "killing pid {} after {} at rip 0x{:X}, rsp 0x{:X}, status {}",
            pid,
            name,
            stack.instruction_pointer.as_u64(),
            stack.stack_pointer.as_u64(),
            status
        ),
        None => {
            // a ring 3 frame without a current process means the scheduler
            // state is inconsistent, there is nobody to kill
            log!(
                crate::io::LogType::ERR,
                "{} from ring 3 with no current process, halting",
                name
            );

            log!(crate::io::LogType::ERR, "Dumping stack frame\n{:#?}", stack);
            loop {}
        }
    }

    scheduling::exit_current(status);
    scheduling::schedule(None);

    // schedule jumps straight into the next process and never returns
    loop {}
}

extern "x86-interrupt" fn divide_error_isr(stack: InterruptStackFrame) {
    handle_fault(&stack, 0, "divide error");
}

extern "x86-interrupt" fn invalid_opcode_isr(stack: InterruptStackFrame) {
    handle_fault(&stack, 6, "invalid opcode");
}

extern "x86-interrupt" fn x87_floating_point_isr(stack: InterruptStackFrame) {
    handle_fault(&stack, 16, "x87 floating point exception");
}

extern "x86-interrupt" fn simd_floating_point_isr(stack: InterruptStackFrame) {
    handle_fault(&stack, 19, "SIMD floating point exception");
}

extern "x86-interrupt" fn stack_segment_fault_isr(stack: InterruptStackFrame, err_code: u64) {
    log!(
        crate::io::LogType::EXCEPTION,
        "Stack segment fault! With error code: 0x{:X}",
        err_code
    );

    handle_fault(&stack, 12, "stack segment fault");
}

extern "x86-interrupt" fn gpf_isr(stack: InterruptStackFrame, err_code: u64) {
    log!(
        crate::io::LogType::EXCEPTION,
        "General protection fault! With error code: 0x{:X}",
        err_code
    );

    handle_fault(&stack, 13, "general protection fault");
}

extern "x86-interrupt" fn page_fault_isr(stack: InterruptStackFrame, err_code: PageFaultErrorCode) {
    let cr2 = Cr2::read().as_u64();
    log!(
        crate::io::LogType::EXCEPTION,
        "Page fault! With error code: 0x{:X}, and cr2: 0x{:X}",
        err_code,
        cr2
    );

    handle_fault(&stack, 14, "page fault");
}

extern "x86-interrupt" fn debug_isr(_stack: InterruptStackFrame) {
    log!(crate::io::LogType::OK, "Debug isr called!");
}

#[naked]
extern "x86-interrupt" fn syscall_trampoline() {
    interrupt_trampoline!(syscall_isr);
}

#[no_mangle]
extern "C" fn syscall_isr(stack: *mut FullInterruptStackFrame) {
    let stack = unsafe { &mut *stack };
    let syscall_number = stack.rax;

    let rax = match syscall_number {
        1 => syscall::exit(stack),
        2 => syscall::write(stack),
        3 => syscall::read(&mut *stack),
        4 => syscall::execute(stack),
        5 => syscall::yld(stack),
        6 => syscall::wait_for_process(stack),
        7 => syscall::read_dir(stack),
        8 => syscall::cd(stack),
        9 => syscall::open(stack),
        10 => syscall::close(stack),
        11 => syscall::truncate(stack),
        // 12 was create, folded into open as O_CREAT
        13 => syscall::mkdir(stack),
        14 => syscall::unlink(stack),
        15 => syscall::rmdir(stack),
        16 => syscall::clock_gettime(stack),
        17 => syscall::nanosleep(stack),
        18 => syscall::brk(stack),
        19 => syscall::sbrk(stack),
        20 => syscall::lseek(stack),
        21 => syscall::fstat(stack),
        22 => syscall::stat(stack),
        23 => syscall::getpid(stack),
        24 => syscall::read_char(&mut *stack),
        25 => syscall::rename(stack),
        _ => {
            log!(
                crate::io::LogType::SYS,
                "Unknown syscall: 0x{:x}",
                syscall_number
            );

            Some(Err(syscall::Errno::NoSys))
        }
    };

    // a handler that returns None descheduled the caller, rax now belongs to
    // whichever process runs next and must be left alone
    if let Some(result) = rax {
        stack.rax = syscall::encode(result);
    }
}

pub fn remap_pic() {
    unsafe {
        // Start PIC init
        io::outb(0x20, 0x11);
        io::outb(0xA0, 0x11);

        // Set vector offset

        // Master: IRQ 0–7 -> vector 0x20
        io::outb(0x21, 0x20);

        // Slave: IRQ 8–15 -> INT 0x28
        io::outb(0xA1, 0x28);

        // Setup chaining
        io::outb(0x21, 0x04);
        io::outb(0xA1, 0x02);

        // Set 8086 mode
        io::outb(0x21, 0x01);
        io::outb(0xA1, 0x01);

        // Unmask all (or use proper mask)
        io::outb(0x21, 0x00);
        io::outb(0xA1, 0x00);
    }
}

pub unsafe fn register_interrupt(vector: usize, handler_addr: usize, is_ring3: bool) {
    let dpl = if is_ring3 {
        PrivilegeLevel::Ring0
    } else {
        PrivilegeLevel::Ring3
    };

    IDT[vector]
        .set_handler_addr(VirtAddr::new(handler_addr as u64))
        .set_privilege_level(dpl);
}

pub unsafe fn init_idt() {
    IDT.breakpoint.set_handler_fn(breakpoint_isr);
    IDT.double_fault
        .set_handler_fn(double_fault_isr)
        .set_stack_index(DOUBLE_FAULT_STACK_INDEX as u16);

    IDT.general_protection_fault.set_handler_fn(gpf_isr);
    IDT.page_fault.set_handler_fn(page_fault_isr);
    IDT.stack_segment_fault
        .set_handler_fn(stack_segment_fault_isr);

    // without these a user program dividing by zero or running a bad
    // instruction would hit an unregistered vector and triple fault
    IDT.divide_error.set_handler_fn(divide_error_isr);
    IDT.invalid_opcode.set_handler_fn(invalid_opcode_isr);

    // CR4.OSXMMEXCPT is set, so an unmasked SSE exception is delivered as #XM
    // rather than #UD. Processes start with every SSE exception masked, so
    // this only fires for one that changed MXCSR itself
    IDT.simd_floating_point
        .set_handler_fn(simd_floating_point_isr);

    IDT.x87_floating_point.set_handler_fn(x87_floating_point_isr);

    IDT[IRQ0 as usize]
        .set_handler_addr(VirtAddr::new(timer_trampoline as u64))
        .set_stack_index(PIT_STACK_INDEX as u16);

    IDT[0x34 as usize].set_handler_fn(debug_isr);

    IDT[0x80 as usize]
        .set_handler_addr(VirtAddr::new(syscall_trampoline as u64))
        .set_stack_index(SYSCALL_STACK_INDEX as u16)
        .set_privilege_level(PrivilegeLevel::Ring3);
}

pub unsafe fn load_idt() {
    // TODO: Initialize interrupt stack
    IDT.load();
}
