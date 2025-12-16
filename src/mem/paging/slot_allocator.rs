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
            init_done: false,
        }
    }

    pub fn alloc<A>(&mut self, pf_alloc: &mut A) -> Option<usize>
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

        // TODO: Check if last_pml1_slot >= 510

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

        let mut pml1_table: Option<PageTable> = None;
        for idx in 0..512 {
            let page = Page::for_address(PAGE_TABLE_REGION_START + (idx * PAGE_SIZE));
            let flags = EntryFlags::PRESENT | EntryFlags::WRITABLE;
            pml1_table = Some(pml4.map_to(
                page.clone(),
                frame.clone(),
                flags,
                pf_alloc,
                self,
                &mut temp_temp_mapper,
            ));

            // 511
            if idx < 512 {
                frame = pf_alloc.falloc().unwrap();
            }
        }

        let pml1_table = pml1_table.unwrap();

        // now we've mapped the entire initial PML1 for the region
        self.pml1_count += 1;

        // now we can map the temp lookup map, but we have to do some near
        // pointer math  to get the allocated VA accessible after the switch

        // temporarily switch to virtual mode to get a temp VA
        self.init_done = true;
        let temp_addr = self.alloc(pf_alloc).unwrap();
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

    fn extend_region(&mut self) {
        // wouldn't I need a master table ref?
    }
}
