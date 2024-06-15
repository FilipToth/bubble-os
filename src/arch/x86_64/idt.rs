use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{mem::MemoryController, print};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_isr);
        idt.double_fault.set_handler_fn(double_fault_isr);
        idt.general_protection_fault.set_handler_fn(gpf_isr);
        // idt.page_fault.set_handler_fn(page_fault_isr);
        idt[0x34 as usize].set_handler_fn(debug_isr);

        idt
    };
}

extern "x86-interrupt" fn breakpoint_isr(_stack: InterruptStackFrame) {
    print!("\n[ EXCEPTION ] Breakpoint interrupt called!\n");
    loop {}
}

extern "x86-interrupt" fn double_fault_isr(stack: InterruptStackFrame, err_code: u64) -> ! {
    print!("\n[ EXCEPTION ] Double fault!\n");
    print!("Dumping stack frame\n{:#?}\n", stack);
    loop {}
}

extern "x86-interrupt" fn gpf_isr(stack: InterruptStackFrame, _err_code: u64) {
    print!("\n[ EXCEPTION ] General protection fault!\n");
    loop {}
}

extern "x86-interrupt" fn page_fault_isr(
    _stack: InterruptStackFrame,
    _err_code: PageFaultErrorCode,
) {
    print!("[ OK ] Page fault!\n");
    loop {}
}

extern "x86-interrupt" fn debug_isr(_stack: InterruptStackFrame) {
    print!("[ OK ] Debug isr called!\n");
    loop {}
}

pub fn load_idt(mem_controller: &mut MemoryController) {
    let double_fault_stack = mem_controller.alloc_stack(1).unwrap();

    IDT.load();
}
