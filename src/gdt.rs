use core::{arch::asm, mem::size_of};

#[repr(C)]
struct GDTSegmentDescriptor {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access_byte: u8,
    limit_flags: u8,
    base_high: u8
}

impl GDTSegmentDescriptor {
    fn encode(base: u32, limit: u32, access: u8, flags: u8) -> GDTSegmentDescriptor {
        let base_low = (base & 0xFFFF) as u16;
        let base_middle = ((base >> 16) & 0xFF) as u8;
        let base_high = ((base >> 24) & 0xFF) as u8;

        let limit_low = (limit & 0xFFFF) as u16;
        let mut limit_flags = ((limit >> 16) & 0xF) as u8;
        limit_flags |= flags & 0xF0;
        
        GDTSegmentDescriptor::new(limit_low, base_low, base_middle, access, limit_flags, base_high)
    }

    fn new(limit_low: u16, base_low: u16, base_middle: u8, access_byte: u8, limit_flags: u8, base_high: u8) -> GDTSegmentDescriptor {
        GDTSegmentDescriptor { limit_low: limit_low, base_low: base_low, base_middle: base_middle, access_byte: access_byte, limit_flags: limit_flags, base_high: base_high }
    }
}

#[repr(C, align(0x1000))]
pub struct GDT {
    null_descriptor: GDTSegmentDescriptor,
    kernel_mode_code_segment: GDTSegmentDescriptor,
    kernel_mode_data_segment: GDTSegmentDescriptor,
    // user_mode_code_segment: GDTSegmentDescriptor,
    // user_mode_data_segment: GDTSegmentDescriptor,
    // task_state_segment: GDTSegmentDescriptor
}

#[repr(C, packed(2))]
struct GDTDescriptor {
    size: u16,
    offset: u64
}

lazy_static! {
    pub static ref GLOB_DESC_TABLE: GDT = GDT {
        null_descriptor: GDTSegmentDescriptor::new(0, 0, 0, 0, 0, 0),
        kernel_mode_code_segment: GDTSegmentDescriptor::encode(0x00400000, 0x003FFFFF, 0x9A, 0xC),
        kernel_mode_data_segment: GDTSegmentDescriptor::encode(0x00800000, 0x003FFFFF, 0x92, 0xC),
        // task_state_segment: GDTSegmentDescriptor::encode(base, limit, 0x89, 0x0)
    };
}

pub fn load_gdt() {
    unsafe {
        let descriptor = GDTDescriptor {
            size: size_of::<GDT>() as u16 - 1,
            offset: (&*GLOB_DESC_TABLE as *const _) as u64
        };
    
        asm!("lgdt [{}]", in(reg) &descriptor, options(readonly, nostack, preserves_flags));
    }
}