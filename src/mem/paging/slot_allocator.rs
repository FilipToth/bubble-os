use x86_64::structures::paging::page;

use crate::{
    mem::{
        paging::{
            entry::{EntryFlags, PageTableEntry},
            temp_mapper::TempMapper,
            Page, PageTable,
        },
        PageFrame, PageFrameAllocator, PAGE_SIZE, PAGE_TABLE_REGION_START,
    },
    print,
};

pub struct PageTableSlotAllocator {
    pub region_start: usize,
    pub first_phys_frame: usize,
    pub pml1_count: usize,
    pub last_pml1_slot: usize,

    // address of the PML2 responsible for the region's PML1s
    pub pml2_addr: usize,

    // false if we're operating in CPU identity paging,
    // true if we've already switched to our page table
    pub init_done: bool,
}

impl PageTableSlotAllocator {
    pub fn new(start: usize) -> Self {
        PageTableSlotAllocator {
            region_start: start,
            first_phys_frame: 0,
            pml1_count: 0,
            last_pml1_slot: 0,
            pml2_addr: 0,
            init_done: false,
        }
    }

    pub fn alloc<A>(&mut self, pf_alloc: &mut A, temp_mapper: &mut TempMapper) -> Option<usize>
    where
        A: PageFrameAllocator,
    {
        let offset = if !self.init_done {
            // we allocated page frames linearly for the region
            // these are physical addresses
            self.first_phys_frame
        } else {
            // virtual addresses
            PAGE_TABLE_REGION_START
        };

        let addr = offset + (self.last_pml1_slot * PAGE_SIZE);
        self.last_pml1_slot += 1;

        // it's importane that this only runs for slot 510 (the second to
        // last slot), because in the extension process, another slot alloc
        // call will be made when allocating the new PML1.
        if self.last_pml1_slot == 510 {
            self.extend_region(pf_alloc, temp_mapper);
        }

        // TODO: Think about whether we can just return this address...

        Some(addr)
    }

    pub fn alloc_master_table<A>(&mut self, pf_alloc: &mut A) -> (PageTable, TempMapper)
    where
        A: PageFrameAllocator,
    {
        // Create a new page table, then map it to the start of our region.
        // Then map all PML1 entries continuously to the next frame

        let mut frame = pf_alloc.falloc().unwrap();
        let mut pml4 = PageTable::new(frame.start_address());

        self.last_pml1_slot += 1;
        self.first_phys_frame = frame.start_address();

        // TODO: This is very very very bad code, but just create a temporary
        // temp_mapper that will not get called, because this is a physical-
        // identity table, just to satisfy argument to `map_to`. Later, we should
        // separate physical and virtually-mapped page tables into separate types
        // and simplfy have a common page table interface.
        let mut temp_temp_mapper = TempMapper::new(0, 0);

        // maybe extract this into the `extend_region` function later

        let mut pml2_table: Option<PageTable> = None;
        let mut pml1_table: Option<PageTable> = None;

        for idx in 0..512 {
            let page = Page::for_address(PAGE_TABLE_REGION_START + (idx * PAGE_SIZE));
            let flags = EntryFlags::PRESENT | EntryFlags::WRITABLE;
            let map_chain = pml4.map_to(
                page.clone(),
                frame.clone(),
                flags,
                pf_alloc,
                self,
                &mut temp_temp_mapper,
            );

            pml2_table = Some(map_chain.pml2);
            pml1_table = Some(map_chain.pml1);

            // 511
            if idx < 512 {
                frame = pf_alloc.falloc().unwrap();
            }
        }

        let pml2_table = pml2_table.unwrap();
        let pml1_table = pml1_table.unwrap();

        self.pml2_addr = pml2_table.addr;

        // now we've mapped the entire initial PML1 for the region
        self.pml1_count += 1;

        // now we can map the temp lookup map, but we have to do some near
        // pointer math  to get the allocated VA accessible after the switch

        // temporarily switch to virtual mode to get a temp VA
        self.init_done = true;
        let temp_addr = self.alloc(pf_alloc, &mut temp_temp_mapper).unwrap();
        self.init_done = false;

        let temp_page = Page::for_address(temp_addr);
        let temp_p1_index = temp_page.p1_index();

        // tables contain physical addresses right now
        let pml1_phys_addr = pml1_table.addr;
        let pml1_addr_offset = pml1_phys_addr - self.first_phys_frame;
        let pml1_virt_addr = PAGE_TABLE_REGION_START + pml1_addr_offset;

        let temp_entry_addr =
            pml1_virt_addr + (temp_p1_index * core::mem::size_of::<PageTableEntry>());

        let temp_mapper = TempMapper::new(temp_addr, temp_entry_addr);
        (pml4, temp_mapper)
    }

    /// Creates a table holding a reference to the PML4.
    pub fn get_pml4(&self) -> PageTable {
        PageTable::new(PAGE_TABLE_REGION_START)
    }

    fn extend_region<A>(&mut self, pf_alloc: &mut A, temp_mapper: &mut TempMapper)
    where
        A: PageFrameAllocator
    {
        let pml1_base = PAGE_TABLE_REGION_START + (PAGE_SIZE * 512 * self.pml1_count);

        // soft clone the active PML4
        let mut pml4 = PageTable::new(PAGE_TABLE_REGION_START);
        
        let start = Page::for_address(pml1_base);
        let end = Page { page_number: start.page_number + 512 };

        for page in Page::range(start, end) {
            let p4i = page.p4_index();
            let p3i = page.p3_index();
            let p2i = page.p2_index();
            let p1i = page.p1_index();

            print!("Extending PT region with page (0x{:X}) => ({}, {}, {}, {})\n", page.start_address(), p4i, p3i, p2i, p1i);

            // this should only make one extra alloc call for the new PML1 slot
            let flags = EntryFlags::PRESENT | EntryFlags::WRITABLE;
            pml4.map(page, flags, pf_alloc, self, temp_mapper);
        }

        self.pml1_count += 1;
    }
}