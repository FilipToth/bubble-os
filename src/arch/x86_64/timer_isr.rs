use core::sync::atomic::Ordering;

use crate::{
    arch, interrupt_trampoline,
    io::serial,
    scheduling::{self, SCHEDULING_ENABLED},
};

use super::registers::FullInterruptStackFrame;

#[naked]
pub extern "x86-interrupt" fn timer_trampoline() {
    interrupt_trampoline!(timer_isr);
}

#[no_mangle]
pub extern "C" fn timer_isr(stack: *mut FullInterruptStackFrame) {
    let sched_enabled = SCHEDULING_ENABLED.load(Ordering::SeqCst);
    arch::x86_64::pit::end_of_interrupt(0);

    let stack = unsafe { &mut *stack };
    if sched_enabled {
        if serial::serial_received() {
            let input = serial::read_serial();
            scheduling::process_input(input);
        }

        scheduling::schedule(&stack);
    }
}
