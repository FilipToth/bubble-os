use x86_64::{
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
    VirtAddr,
};

use crate::{
    arch::x86_64::{gdt::PIT_STACK_INDEX, timer_isr::timer_trampoline},
    interrupt_trampoline,
    io::io,
    print, syscall,
};

use super::registers::FullInterruptStackFrame;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_isr);
        idt.double_fault.set_handler_fn(double_fault_isr);
        idt.general_protection_fault.set_handler_fn(gpf_isr);
        idt.page_fault.set_handler_fn(page_fault_isr);

        unsafe {
            idt[0x20 as usize]
                .set_handler_addr(VirtAddr::new(timer_trampoline as u64))
                .set_stack_index(PIT_STACK_INDEX as u16);
        }

        idt[0x34 as usize].set_handler_fn(debug_isr);

        unsafe {
            idt[0x80 as usize].set_handler_addr(VirtAddr::new(syscall_trampoline as u64));
        }

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

#[naked]
extern "x86-interrupt" fn syscall_trampoline() {
    interrupt_trampoline!(syscall_isr);
}

#[no_mangle]
extern "C" fn syscall_isr(stack: *mut FullInterruptStackFrame) {
    let syscall_number: usize;
    unsafe { core::arch::asm!("", lateout("rax") syscall_number) };

    let stack = unsafe { &mut *stack };
    let rax = match syscall_number {
        1 => syscall::exit(),
        2 => syscall::write(stack),
        3 => syscall::read(stack),
        4 => syscall::execute(stack),
        5 => syscall::yld(stack),
        6 => syscall::wait_for_process(stack),
        7 => {
            // debug syscall
            print!("[ SYS ] Debug syscall, rdi -> 0x{:X}\n", stack.rdi);
            None
        }
        _ => {
            print!("[ SYS ] Unknown syscall: 0x{:x}\n", syscall_number);
            Some(0)
        }
    };

    // set return value
    if let Some(rax) = rax {
        stack.rax = rax;
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
