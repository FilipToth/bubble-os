use x86_64::{
    instructions::tables::load_tss,
    registers::segmentation::{Segment, CS, DS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

use crate::{mem::GLOBAL_MEMORY_CONTROLLER, print};

pub struct Selectors {
    tss: SegmentSelector,
    code: SegmentSelector,
    data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
}

pub static PIT_STACK_INDEX: usize = 0;
pub static SYSCALL_STACK_INDEX: usize = 1;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        let pit_stack = alloc_ist_stack();
        tss.interrupt_stack_table[PIT_STACK_INDEX] = VirtAddr::new(pit_stack);

        let syscall_stack = alloc_ist_stack();
        tss.interrupt_stack_table[SYSCALL_STACK_INDEX] = VirtAddr::new(syscall_stack);

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

    match mc.alloc_stack(16) {
        Some(s) => s.top as u64,
        None => {
            print!("[ ERR ] Couldn't allocate IST stack!\n");
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
