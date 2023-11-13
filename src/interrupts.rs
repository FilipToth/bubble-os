use core::sync::atomic::{AtomicPtr, Ordering};
use core::mem::size_of;
use core::arch::asm;

use crate::gdt;
use crate::print;

static mut IDT_REG: IDTR = IDTR {
    limit: 0,
    base: 0
};

lazy_static ! {
    static ref IDT: InterruptDT = InterruptDT::new();
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IDTEntry {
    isr_lower: u16, // lower 16 bits of ISR address
    kernel_cs: u16, // GDT Segement selector that will be loaded into code segment before the ISR call
    ist: u8, // ist in the tss that the rsp will be loaded into, 0 for now
    attributes: u8,
    isr_middle: u16, // middle 16 bits of ISR address
    isr_higher: u32, // higher 32 bits of ISR address
    reserved: u32 // 0
}

impl IDTEntry {
    fn undefined() -> IDTEntry {
        IDTEntry {
            isr_lower: 0,
            kernel_cs: 0,
            ist: 0,
            attributes: 0, // minimum attribute field
            isr_middle: 0,
            isr_higher: 0,
            reserved: 0
        }
    }
}

#[repr(C, packed(2))]
struct IDTR {
    limit: u16,
    base: u64
}

#[repr(C, align(16))]
struct InterruptDT {
    divide_error: IDTEntry,
    interrupts: [IDTEntry; 256 - 1]
}

impl InterruptDT {
    fn new() -> InterruptDT {
        let mut idt = InterruptDT {
            divide_error: IDTEntry::undefined(),
            interrupts: [IDTEntry::undefined(); 256 - 1]
        };


        for vector in 0..=255 {
            unsafe {
                let descriptor = set_idt_descriptor(test_isr as *const (), 0x8E);
                idt.set_descriptor(descriptor, vector as usize);
            }
        }

        return idt;
    }

    fn set_descriptor(&mut self, descriptor: IDTEntry, vector: usize) {
        match vector {
            1 => self.divide_error = descriptor,
            _ => self.interrupts[vector] = descriptor
        }
    }
}

#[repr(C)]
struct InterruptStackFrame {
    _instruction_ptr: u64,
    _code_seg: u64,
    _cpu_flags: u64,
    _stack_ptr: u64,
    _stack_segment: u64
}

#[cfg(all(feature = "instructions", feature = "abi_x86_interrupt"))]
fn test_isr(_: InterruptStackFrame) {
    print!("moo! foobar");
    loop { }
}

unsafe fn set_idt_descriptor(isr: *const (), flags: u8) -> IDTEntry {
    let mut descriptor = IDTEntry::undefined();
    let isr_addr = isr as u64;

    descriptor.isr_lower = (isr_addr & 0xFFFF) as u16;
    descriptor.kernel_cs = (&*gdt::GLOB_DESC_TABLE as *const _) as u16;
    descriptor.ist = 0;
    descriptor.attributes = flags;
    descriptor.isr_middle = ((isr_addr>> 16) & 0xFFFF) as u16;
    descriptor.isr_higher = ((isr_addr >> 32) & 0xFFFFFFFF) as u32;
    descriptor.reserved = 0;

    return descriptor;
}

pub unsafe fn init_idt() {
    IDT_REG.base = (&IDT as *const _) as u64;
    IDT_REG.limit = (size_of::<IDTEntry>() * 256 - 1) as u16;

    let addr = (&IDT_REG as *const _) as u64;
    let idtr_ptr = addr as *const IDTR;
    let idtr_ref = idtr_ptr.as_ref().unwrap();

    print!("idtr_ref: {}\n", (&IDT as *const _) as u64);

    asm!("lidt [{}]", in(reg) &IDT_REG, options(readonly, nostack, preserves_flags));

    print!("IDT initialized\n");
}