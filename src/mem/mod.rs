pub mod heap;
mod linked_list_allocator;
pub mod paging;
mod simple_page_frame_allocator;

use multiboot2::BootInformation;

use crate::{print, mem::{paging::{remap_kernel, Page, entry::EntryFlags}, heap::{HEAP_START, HEAP_SIZE}}};

pub use self::simple_page_frame_allocator::SimplePageFrameAllocator;

pub type VirtualAddress = usize;
pub type PhysicalAddress = usize;

pub static PAGE_SIZE: usize = 4096;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageFrame {
    pub frame_number: usize,
}

impl PageFrame {
    fn from_address(addr: usize) -> PageFrame {
        let number = addr / PAGE_SIZE;
        PageFrame {
            frame_number: number,
        }
    }

    pub fn start_address(&self) -> PhysicalAddress {
        self.frame_number * PAGE_SIZE
    }

    fn clone(&self) -> PageFrame {
        PageFrame {
            frame_number: self.frame_number,
        }
    }

    fn range(start: PageFrame, end: PageFrame) -> PageFrameIter {
        PageFrameIter {
            start: start,
            end: end,
        }
    }
}

struct PageFrameIter {
    start: PageFrame,
    end: PageFrame,
}

impl Iterator for PageFrameIter {
    type Item = PageFrame;

    fn next(&mut self) -> Option<PageFrame> {
        if self.start > self.end {
            return None;
        }

        let frame = self.start.clone();
        self.start.frame_number += 1;
        Some(frame)
    }
}

pub trait PageFrameAllocator {
    fn falloc(&mut self) -> Option<PageFrame>;
    fn free(&mut self, frame: PageFrame);
}

pub fn init(boot_info: &BootInformation) {
    let map_tag = boot_info.memory_map_tag().unwrap();
    print!("\n[ OK ] Kernel Init Done, Entering Rust 64-Bit Mode\n");

    let elf_sections = boot_info.elf_sections().unwrap();
    let kernel_start = elf_sections
        .clone()
        .map(|s| s.start_address())
        .min()
        .unwrap();
    let kernel_end = elf_sections
        .clone()
        .map(|s| s.start_address() + s.size())
        .max()
        .unwrap();

    let multiboot_start = boot_info.start_address();
    let multiboot_end = multiboot_start + (boot_info.total_size() as usize);

    print!(
        "[ OK ] Identified kernel at start: 0x{:x} end: 0x{:x}\n",
        kernel_start, kernel_end
    );
    print!(
        "[ OK ] Identified multiboot info at start: 0x{:x} end: 0x{:x}\n",
        multiboot_start, multiboot_end
    );

    // memory

    // for some reason when getting the last memory area,
    // it's always padded to 4GB, the second last area
    // actually corresponds to the memory available

    let mem_areas = map_tag.memory_areas();
    let memory_end = mem_areas[mem_areas.len() - 2].end_address();

    print!("[ OK ] Memory end: 0x{:x}\n", memory_end);

    let mut allocator = SimplePageFrameAllocator::new(multiboot_end as usize, memory_end as usize);

    // for some reason I have to allocate
    // and empty page here or else it
    // panics and faults

    let _ = allocator.falloc().unwrap();

    let mut active_table = remap_kernel(&mut allocator, &boot_info);
    print!("[ OK ] RAN KERNEL REMAP\n");

    // map heap pages
    let heap_start = Page::for_address(HEAP_START);
    let heap_end = Page::for_address(HEAP_START + HEAP_SIZE - 1);

    for page in Page::range(heap_start, heap_end) {
        active_table.map(page, EntryFlags::PRESENT | EntryFlags::WRITABLE, &mut allocator);
    }
}
