use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::print;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.page_fault.set_handler_fn(page_fault_isr);
        idt.general_protection_fault.set_handler_fn(gp_fault_isr);
        idt.double_fault.set_handler_fn(double_fault_isr);
        idt.divide_error.set_handler_fn(divide_error_isr);

        idt[0x34 as usize].set_handler_fn(proof_of_concept_isr);
        
        idt
    };
}

extern "x86-interrupt" fn page_fault_isr(stack: InterruptStackFrame, err: PageFaultErrorCode) {
    print!(
        "[ Error ] Page Fault\n    at IP: {}\n    with page fault err code: {:?}\n",
        stack.instruction_pointer.as_u64(),
        err
    );

    loop {};
}

extern "x86-interrupt" fn gp_fault_isr(stack: InterruptStackFrame, err: u64) {
    print!(
        "[ Error ] General Protection Fault\n    at IP: {}\n    with fault err: {:?}\n",
        stack.instruction_pointer.as_u64(),
        err
    );

    loop {};
}

extern "x86-interrupt" fn double_fault_isr(stack: InterruptStackFrame, err: u64) -> ! {
    print!(
        "[ Error ] Double Fault\n    at IP: {}\n    with fault err: {:?}\n",
        stack.instruction_pointer.as_u64(),
        err
    );

    loop {};
}

extern "x86-interrupt" fn divide_error_isr(stack: InterruptStackFrame) {
    print!(
        "[ Error ] Divide Error\n    at IP: {}\n",
        stack.instruction_pointer.as_u64(),
    );

    loop {};
}

extern "x86-interrupt" fn proof_of_concept_isr(_stack: InterruptStackFrame) {
    print!("Proof of concept interrupt called!!!!\n");
}

pub fn init_interrupts() {
    IDT.load()
}
