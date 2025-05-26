use x86_64::{
    instructions::tables::load_tss,
    registers::segmentation::{Segment, CS, DS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    }, VirtAddr,
};

use crate::{mem::GLOBAL_MEMORY_CONTROLLER, print};

struct Selectors {
    tss: SegmentSelector,
    code: SegmentSelector,
    data: SegmentSelector,
}

pub static PIT_STACK: [u64; 4096] = [0; 4096];
pub static PIT_STACK_INDEX: usize = 0;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        /*
        let pit_stack_start = &PIT_STACK as *const _ as usize;
        let pit_stack_end = pit_stack_start + core::mem::size_of::<u64>() * 4096;

        // STACK ISN'T MAPPED!!!
        let x = (pit_stack_start + 0xF0) as *mut u8;
        unsafe { *x = 0xFF };
        */

        let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
        let mc = mc.as_mut().unwrap();

        let stack = match mc.alloc_stack(16) {
            Some(s) => s,
            None => {
                print!("[ ERR ] Couldn't allocate IST stack!\n");
                panic!();
            }
        };

        let x = (stack.bottom + 0xF0) as *mut u8;
        unsafe { *x = 0xFF };
        let y = unsafe { *x };
        print!("[ TSS ] y: 0x{:X}\n", y);

        print!("[ TSS ] Setting PIT Stack in TSS to: (0x{:x}, 0x{:x})\n", stack.bottom, stack.top);
        tss.interrupt_stack_table[PIT_STACK_INDEX] = VirtAddr::new(stack.top as u64);

        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let tss = gdt.add_entry(Descriptor::tss_segment(&TSS));
        let code = gdt.add_entry(Descriptor::kernel_code_segment());
        let data = gdt.add_entry(Descriptor::kernel_data_segment());

        let selectors = Selectors {
            tss: tss,
            code: code,
            data: data,
        };

        (gdt, selectors)
    };
}

pub fn init_gdt() {
    GDT.0.load();

    unsafe {
        CS::set_reg(GDT.1.code);
        DS::set_reg(GDT.1.data);
        load_tss(GDT.1.tss);
    };
}
