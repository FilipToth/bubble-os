use x86_64::{
    instructions::tables::load_tss,
    registers::segmentation::{Segment, CS, DS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
};

struct Selectors {
    tss: SegmentSelector,
    code: SegmentSelector,
    data: SegmentSelector,
}

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let tss = TaskStateSegment::new();
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
