use x86_64::{
    instructions::tables::load_tss,
    registers::segmentation::{Segment, CS, DS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

use crate::log;
use crate::mem::GLOBAL_MEMORY_CONTROLLER;

pub struct Selectors {
    tss: SegmentSelector,
    code: SegmentSelector,
    data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
}

pub static PIT_STACK_INDEX: usize = 0;
pub static SYSCALL_STACK_INDEX: usize = 1;
pub static DOUBLE_FAULT_STACK_INDEX: usize = 2;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        let pit_stack = alloc_ist_stack();
        tss.interrupt_stack_table[PIT_STACK_INDEX] = VirtAddr::new(pit_stack);

        let syscall_stack = alloc_ist_stack();
        tss.interrupt_stack_table[SYSCALL_STACK_INDEX] = VirtAddr::new(syscall_stack);

        let double_fault_stack = alloc_ist_stack();
        tss.interrupt_stack_table[DOUBLE_FAULT_STACK_INDEX] = VirtAddr::new(double_fault_stack);

        // the stack the CPU switches to when an exception without a
        // dedicated IST stack arrives from ring 3; without it the CPU
        // would push the exception frame to address 0 and triple fault
        let ring0_stack = alloc_ist_stack();
        tss.privilege_stack_table[0] = VirtAddr::new(ring0_stack);

        tss
    };
}

lazy_static! {
    pub static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let tss = gdt.add_entry(Descriptor::tss_segment(&TSS));
        let code = gdt.add_entry(Descriptor::kernel_code_segment());
        let data = gdt.add_entry(Descriptor::kernel_data_segment());

        let user_code = gdt.add_entry(Descriptor::user_code_segment());
        let user_data = gdt.add_entry(Descriptor::user_data_segment());

        let selectors = Selectors {
            tss: tss,
            code: code,
            data: data,
            user_code: user_code,
            user_data: user_data,
        };

        (gdt, selectors)
    };
}

fn alloc_ist_stack() -> u64 {
    let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
    let mc = mc.as_mut().unwrap();

    match mc.alloc_stack(16, false) {
        Some(s) => s.top as u64,
        None => {
            log!(crate::io::LogType::ERR, "Couldn't allocate IST stack!");
            panic!();
        }
    }
}

pub fn init_gdt() {
    GDT.0.load();

    unsafe {
        CS::set_reg(GDT.1.code);
        DS::set_reg(GDT.1.data);
        load_tss(GDT.1.tss);
    };
}
