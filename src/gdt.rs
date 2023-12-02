use x86_64::instructions::segmentation::{CS, DS};
use x86_64::registers::segmentation::Segment;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};

struct SegmentSelectors {
    kernel_code: SegmentSelector,
    kernel_data: SegmentSelector,
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, SegmentSelectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let kernel_code = gdt.add_entry(Descriptor::kernel_code_segment());
        let kernel_data = gdt.add_entry(Descriptor::kernel_data_segment());

        let selectors = SegmentSelectors {
            kernel_code: kernel_code,
            kernel_data: kernel_data,
        };

        (gdt, selectors)
    };
}

pub unsafe fn load_gdt() {
    GDT.0.load();

    // reload segment registers
    let selectors = &GDT.1;
    CS::set_reg(SegmentSelector::new(0x08, x86_64::PrivilegeLevel::Ring0)); // selectors.kernel_code
    DS::set_reg(SegmentSelector::new(0x08, x86_64::PrivilegeLevel::Ring0)); // selectors.kernel_data
}
