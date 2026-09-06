use core::sync::atomic::Ordering;

use x86_64::instructions::interrupts;

use crate::{
    arch, interrupt_trampoline,
    io::{console, serial},
    scheduling::{self, SCHEDULING_ENABLED},
    time,
};

use super::registers::FullInterruptStackFrame;

#[naked]
pub extern "x86-interrupt" fn timer_trampoline() {
    interrupt_trampoline!(timer_isr);
}

#[no_mangle]
pub extern "C" fn timer_isr(stack: *mut FullInterruptStackFrame) {
    interrupts::disable();
    time::tick();

    let sched_enabled = SCHEDULING_ENABLED.load(Ordering::SeqCst);
    arch::x86_64::pit::end_of_interrupt(0);

    if sched_enabled {
        // drain the whole FIFO rather than one byte per tick. The 16550 holds
        // sixteen bytes and its own interrupt is masked, so taking one per
        // tick capped input at PIT_HZ bytes per second and lost the rest
        // inside the UART
        let mut received = false;
        while serial::serial_received() {
            console::push(serial::read_serial());
            received = true;
        }

        if received {
            scheduling::wake_input_waiters();
        }

        let stack = unsafe { &mut *stack };
        scheduling::schedule(Some(&stack));
    }
}
