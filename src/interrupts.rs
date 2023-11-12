use core::sync::atomic::{AtomicPtr, Ordering};
use core::mem::size_of;
use core::arch::asm;

use crate::print;

static mut IDT_REG: IDTR = IDTR {
    limit: 0,
    base: 0
};

static IDT: AtomicPtr<[IDTEntry; 256]> = AtomicPtr::new(core::ptr::null_mut());

#[repr(C, packed)]
struct IDTEntry {
    isr_lower: u16, // lower 16 bits of ISR address
    kernel_cs: u16, // GDT Segement selector that will be loaded into code segment before the ISR call
    ist: u8, // ist in the tss that the rsp will be loaded into, 0 for now
    attributes: u8,
    isr_middle: u16, // middle 16 bits of ISR address
    isr_higher: u32, // higher 32 bits of ISR address
    reserved: u32 // 0
}

#[repr(C, packed)]
struct IDTR {
    limit: u16,
    base: u64
}

unsafe fn set_idt_descriptor(vector: u8, isr: *const (), flags: u8) {
    let idt = IDT.load(Ordering::SeqCst);
    let descriptor = &mut (*idt)[vector as usize];
    let isr_addr = isr as u64;

    descriptor.isr_lower = (isr_addr & 0xFFFF) as u16;
    descriptor.kernel_cs = 0; // GDT_OFFSET_KERNEL_CS... TODO
    descriptor.ist = 0;
    descriptor.attributes = flags;
    descriptor.isr_middle = ((isr_addr>> 16) & 0xFFFF) as u16;
    descriptor.isr_higher = ((isr_addr >> 32) & 0xFFFFFFFF) as u32;
    descriptor.reserved = 0;
}

#[cfg(all(feature = "instructions", feature = "abi_x86_interrupt"))]
fn test_isr() {
    print!("moo! foobar");
}

pub unsafe fn init_idt() {
    let idt = IDT.load(Ordering::SeqCst);

    IDT_REG.base = (&idt as *const _) as u64;
    IDT_REG.limit = (size_of::<IDTEntry>() * 256 - 1) as u16;

    for vector in 0..=255 {
        set_idt_descriptor(vector, test_isr as *const (), 0x8E);
    }

    asm!("lidt [{}]", in(reg) &idt, options(readonly, nostack, preserves_flags));
}