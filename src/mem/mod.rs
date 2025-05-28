pub mod heap;
mod linked_list_allocator;
mod page_frame;
pub mod paging;
mod region;
mod simple_page_frame_allocator;
mod stack;
mod stack_allocator;

use multiboot2::BootInformation;
use paging::ActivePageTable;
use spin::Mutex;
use stack::Stack;
use stack_allocator::StackAllocator;

use crate::{
    mem::{
        heap::{HEAP_SIZE, HEAP_START},
        paging::{entry::EntryFlags, remap_kernel, Page},
    },
    print,
};

pub use self::page_frame::{PageFrame, PageFrameAllocator, PAGE_SIZE};
pub use self::region::Region;
pub use self::simple_page_frame_allocator::SimplePageFrameAllocator;

pub type VirtualAddress = usize;
pub type PhysicalAddress = usize;

pub struct MemoryController {
    pub active_table: ActivePageTable,
    pub frame_allocator: SimplePageFrameAllocator,
    pub stack_allocator: StackAllocator,
}

pub static GLOBAL_MEMORY_CONTROLLER: Mutex<Option<MemoryController>> = Mutex::new(None);

impl MemoryController {
    fn new(
        active_table: ActivePageTable,
        frame_allocator: SimplePageFrameAllocator,
        stack_allocator: StackAllocator,
    ) -> MemoryController {
        MemoryController {
            active_table: active_table,
            frame_allocator: frame_allocator,
            stack_allocator: stack_allocator,
        }
    }

    pub fn alloc_stack(&mut self, pages_to_alloc: usize) -> Option<Stack> {
        self.stack_allocator.alloc(
            &mut self.active_table,
            &mut self.frame_allocator,
            pages_to_alloc,
        )
    }

    /// Maps a range of pages to their exact corresponding page frames
    ///
    /// ## Arguments
    ///
    /// - `start` the start page
    /// - `end` the end page
    /// - `flags` the page table entry flags to be applied
    pub fn identity_map(&mut self, start: PageFrame, end: PageFrame, flags: EntryFlags) {
        let allocator = &mut self.frame_allocator;
        let table = &mut self.active_table;

        let page = Page::for_address(start.start_address());
        let unused = table.is_unused(page, allocator);
        if !unused {
            // already mapped
            return;
        }

        let range = PageFrame::range(start, end);
        for frame in range {
            table.map_identity(frame, flags, allocator);
        }
    }

    pub fn translate_to_physical(&mut self, addr: usize) -> Option<usize> {
        self.active_table.translate_to_phys(addr)
    }

    /// Maps a range of pages to unused page frames
    ///
    /// ## Arguments
    ///
    /// - `start` the start page
    /// - `end` the end page
    /// - `flags` the page table entry flags to be applied
    pub fn map(&mut self, start: Page, end: Page, flags: EntryFlags) {
        let allocator = &mut self.frame_allocator;
        let table = &mut self.active_table;

        for page in Page::range(start, end) {
            let frame = allocator.falloc().expect("Out of memory");
            table.map_to(page, frame, flags, allocator);
        }
    }

    /// Unmaps a range of pages and frees page frames
    ///
    /// ## Arguments
    ///
    /// - `start` the start page
    /// - `end` the end page
    pub fn unmap(&mut self, start: Page, end: Page) {
        let allocator = &mut self.frame_allocator;
        let table = &mut self.active_table;

        for page in Page::range(start, end) {
            table.unmap(page, allocator);
        }
    }
}

pub fn init(boot_info: &BootInformation) {
    let map_tag = boot_info.memory_map_tag().unwrap();
    print!("\n[ OK ] Kernel Init Done, Entered Rust 64-Bit Mode\n");

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
        active_table.map(
            page,
            EntryFlags::PRESENT | EntryFlags::WRITABLE,
            &mut allocator,
        );
    }

    let stack_allocator = {
        let stack_start = heap_end + 1;
        let stack_end = stack_start + 100;
        let stack_range = Page::range(stack_start, stack_end);

        StackAllocator::new(stack_range)
    };

    let controller = MemoryController::new(active_table, allocator, stack_allocator);

    let mut guard = GLOBAL_MEMORY_CONTROLLER.lock();
    *guard = Some(controller);
}
