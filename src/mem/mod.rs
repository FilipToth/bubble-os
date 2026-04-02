pub mod heap;
mod linked_list_allocator;
mod page_frame;
pub mod paging;
mod region;
mod simple_page_frame_allocator;
mod stack;
mod stack_allocator;

use multiboot2::BootInformation;
use spin::Mutex;
use stack_allocator::StackAllocator;
use x86_64::{
    PhysAddr, instructions::tlb, registers::control::{Cr3, Cr3Flags}, structures::paging::PhysFrame
};

use crate::{
    mem::{
        heap::{HEAP_SIZE, HEAP_START},
        paging::{
            entry::EntryFlags, map_kernel, slot_allocator::PageTableSlotAllocator,
            temp_mapper::TempMapper, Page, PageTable,
        },
    },
    print,
};

pub use self::page_frame::{PageFrame, PageFrameAllocator, PAGE_SIZE};
pub use self::region::Region;
pub use self::simple_page_frame_allocator::SimplePageFrameAllocator;
pub use self::stack::Stack;

pub type VirtualAddress = usize;
pub type PhysicalAddress = usize;

pub static GLOBAL_MEMORY_CONTROLLER: Mutex<Option<MemoryController>> = Mutex::new(None);

pub const PAGE_TABLE_REGION_START: usize = 0x0000_6BCF_0000_0000;

pub struct MemoryController {
    pub active_table: PageTable,
    pub kernel_table: PageTable,
    pub frame_allocator: SimplePageFrameAllocator,
    pub stack_allocator: StackAllocator,
    pub slot_allocator: PageTableSlotAllocator,
    pub temp_mapper: TempMapper,
}

impl MemoryController {
    fn new(
        active_table: PageTable,
        kernel_table: PageTable,
        frame_allocator: SimplePageFrameAllocator,
        stack_allocator: StackAllocator,
        slot_allocator: PageTableSlotAllocator,
        temp_mapper: TempMapper,
    ) -> MemoryController {
        MemoryController {
            active_table: active_table,
            kernel_table: kernel_table,
            frame_allocator: frame_allocator,
            stack_allocator: stack_allocator,
            slot_allocator: slot_allocator,
            temp_mapper: temp_mapper,
        }
    }

    pub fn alloc_stack(&mut self, pages_to_alloc: usize, user: bool) -> Option<Stack> {
        let flags = if user {
            EntryFlags::WRITABLE | EntryFlags::RING3_ACCESSIBLE
        } else {
            EntryFlags::WRITABLE
        };

        self.stack_allocator.alloc(
            &mut self.active_table,
            &mut self.frame_allocator,
            &mut self.slot_allocator,
            &mut self.temp_mapper,
            pages_to_alloc,
            flags,
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
        let temp_mapper = &mut self.temp_mapper;
        let table = &mut self.active_table;

        let page = Page::for_address(start.start_address());
        let unused = table.is_unused(page, temp_mapper);
        if !unused {
            // already mapped
            return;
        }

        let range = PageFrame::range(start, end);
        for frame in range {
            table.map_identity(
                frame,
                flags,
                allocator,
                &mut self.slot_allocator,
                &mut self.temp_mapper,
            );
        }
    }

    pub fn translate_to_physical(&mut self, addr: usize) -> Option<PageFrame> {
        self.active_table
            .translate_to_phys(addr, &mut self.temp_mapper)
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
            table.map_to(
                page,
                frame,
                flags,
                allocator,
                &mut self.slot_allocator,
                &mut self.temp_mapper,
            );
        }
    }

    /// Unmaps a range of pages and frees page frames
    ///
    /// ## Arguments
    ///
    /// - `start` the start page
    /// - `end` the end page
    pub fn unmap(&mut self, start: Page, end: Page) {
        let temp_mapper = &mut self.temp_mapper;
        let table = &mut self.active_table;

        for page in Page::range(start, end) {
            table.unmap(page, temp_mapper);
        }
    }

    /// Clones the kernel base page table, keeping all
    /// kernel table mappings active in the sub-table
    pub fn clone_kernel_table(&mut self) -> Option<PageTable> {
        let slot = self
            .slot_allocator
            .alloc(&mut self.frame_allocator, &mut self.temp_mapper)?;

        let active_ptr = self.active_table.addr as *mut [u8; PAGE_SIZE];
        let new_ptr = slot as *mut [u8; PAGE_SIZE];
        unsafe { active_ptr.copy_to_nonoverlapping(new_ptr, 1) };

        let new_table = PageTable::new(slot);
        Some(new_table)
    }

    /// Switches the active page table into a the provided page table.
    /// Also changes the active table reference in the memory controller
    /// to the new table.
    /// 
    /// ## Arguments
    /// 
    ///  - `new_table`: the page table to switch to
    /// 
    /// ## Returns
    /// The old page table if successful, None if unsuccessful
    pub fn switch_table(
        &mut self,
        new_table: &PageTable
    ) -> Option<PageTable> {
        let addr = new_table.addr;
        if addr < PAGE_TABLE_REGION_START {
            // may be physical address or invalid page table
            return None;
        }

        let Some(phys_frame) = self.active_table.translate_to_phys(addr, &mut self.temp_mapper) else {
            return None;
        };

        let phys_addr = phys_frame.start_address() as u64;
        let phys_addr = PhysAddr::new(phys_addr);
        let phys_frame = PhysFrame::from_start_address(phys_addr)
            .expect("Cannot create cr3 new frame swap address.");

        print!("Switching cr3 to 0x{:X}\n", addr);
        unsafe { Cr3::write(phys_frame, Cr3Flags::empty()) };

        // TODO: In the future, think about how to optimize the TLB here, maybe
        // we don't have to flush the entire thing, just the user-sections,
        // assuming this is switching between kernel->user tables
        tlb::flush_all();

        let old_table = self.active_table.clone();
        self.active_table = new_table.clone();

        Some(old_table)
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

    let mut slot_allocator = PageTableSlotAllocator::new(PAGE_TABLE_REGION_START);
    let (mut pml4, mut temp) = slot_allocator.alloc_master_table(&mut allocator);

    map_kernel(
        &mut allocator,
        &mut slot_allocator,
        &mut pml4,
        boot_info,
        &mut temp,
    );

    // switch to new pml4
    let phys_addr = PhysAddr::new(pml4.addr as u64);
    let phys_frame = PhysFrame::from_start_address(phys_addr)
        .expect("Cannot create cr3 new frame swap address.");

    unsafe { Cr3::write(phys_frame, Cr3Flags::empty()) };

    // switch slot allocator into virtual mode
    slot_allocator.init_done = true;

    // reuse the table struct, just set the address
    // where it's mapped virtually
    pml4.addr = PAGE_TABLE_REGION_START;

    // map heap pages
    let heap_start = Page::for_address(HEAP_START);
    let heap_end = Page::for_address(HEAP_START + HEAP_SIZE - 1);

    for page in Page::range(heap_start, heap_end) {
        pml4.map(
            page,
            EntryFlags::WRITABLE,
            &mut allocator,
            &mut slot_allocator,
            &mut temp,
        );
    }

    let stack_allocator = {
        let stack_start = heap_end + 1;
        let stack_end = stack_start + 100;
        let stack_range = Page::range(stack_start, stack_end);

        StackAllocator::new(stack_range)
    };

    let controller = MemoryController::new(pml4.clone(), pml4, allocator, stack_allocator, slot_allocator, temp);

    let mut guard = GLOBAL_MEMORY_CONTROLLER.lock();
    *guard = Some(controller);
}
