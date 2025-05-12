use core::sync::atomic::Ordering;

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{
    arch,
    io::io,
    print,
    scheduling::{self, SCHEDULING_ENABLED},
};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_isr);
        idt.double_fault.set_handler_fn(double_fault_isr);
        idt.general_protection_fault.set_handler_fn(gpf_isr);
        idt.page_fault.set_handler_fn(page_fault_isr);

        idt[0x20 as usize].set_handler_fn(timer_isr);
        idt[0x34 as usize].set_handler_fn(debug_isr);
        idt[0x80 as usize].set_handler_fn(syscall_isr);

        idt
    };
}

extern "x86-interrupt" fn breakpoint_isr(_stack: InterruptStackFrame) {
    print!("\n[ EXCEPTION ] Breakpoint interrupt called!\n");
    loop {}
}

extern "x86-interrupt" fn double_fault_isr(stack: InterruptStackFrame, err_code: u64) -> ! {
    print!("\n[ EXCEPTION ] Double fault, err_code: 0x{:x}\n", err_code);
    print!("Dumping stack frame\n{:#?}\n", stack);
    loop {}
}

extern "x86-interrupt" fn gpf_isr(_stack: InterruptStackFrame, _err_code: u64) {
    print!("\n[ EXCEPTION ] General protection fault!\n");
    loop {}
}

extern "x86-interrupt" fn page_fault_isr(
    stack: InterruptStackFrame,
    _err_code: PageFaultErrorCode,
) {
    print!("[ EXCEPTION ] Page fault!\n");
    print!("Dumping stack frame\n{:#?}\n", stack);
    loop {}
}

extern "x86-interrupt" fn debug_isr(_stack: InterruptStackFrame) {
    print!("[ OK ] Debug isr called!\n");
}

extern "x86-interrupt" fn timer_isr(stack: InterruptStackFrame) {
    let sched_enabled = SCHEDULING_ENABLED.load(Ordering::SeqCst);
    arch::x86_64::pit::end_of_interrupt(0);

    if sched_enabled {
        scheduling::schedule(&stack);
    }
}

extern "x86-interrupt" fn syscall_isr(_stack: InterruptStackFrame) {
    let syscall_number: usize;
    unsafe { core::arch::asm!("", lateout("rax") syscall_number) };

    match syscall_number {
        1 => {
            // exit syscall
        }
        _ => print!("[ SYS ] Unknown syscall: 0x{:x}\n", syscall_number),
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

pub fn load_idt() {
    // TODO: Initialize interrupt stack
    IDT.load();
}
