use core::ops::IndexMut;

use x86_64::structures::paging::frame;

use crate::{
    mem::{
        paging::{
            entry::{EntryFlags, PageTableEntry},
            slot_allocator::PageTableSlotAllocator,
            temp_mapper::TempMapper,
            Page,
        },
        PageFrame, PageFrameAllocator, PAGE_TABLE_REGION_START,
    },
    print,
};

pub struct PageTable {
    pub addr: usize,
}

impl PageTable {
    pub fn new(addr: usize) -> Self {
        Self { addr: addr }
    }

    /// Maps a page to its exact corresponding page frame
    ///
    /// ## Arguments
    ///
    /// - `frame` the page frame for the page to be mapped on to
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map_identity<A>(
        &mut self,
        frame: PageFrame,
        flags: EntryFlags,
        alloc: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
    ) where
        A: PageFrameAllocator,
    {
        let page = Page::for_address(frame.start_address());
        self.map_to(page, frame, flags, alloc, slot_alloc, temp_mapper);
    }

    /// Maps a range of pages to unused page frames
    ///
    /// ## Arguments
    ///
    /// - `start` the start page frame for the mapping
    /// - `end` the end page frame for the mapping
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map_range<A>(
        &mut self,
        start: Page,
        end: Page,
        flags: EntryFlags,
        alloc: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
    ) where
        A: PageFrameAllocator,
    {
        let range = Page::range(start, end);
        for page in range {
            self.map(page, flags, alloc, slot_alloc, temp_mapper);
        }
    }

    /// Maps a range of pages to its exact corresponding page frame
    ///
    /// ## Arguments
    ///
    /// - `start` the start page frame for the mapping
    /// - `end` the end page frame for the mapping
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map_range_identity<A>(
        &mut self,
        start: PageFrame,
        end: PageFrame,
        flags: EntryFlags,
        alloc: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
    ) where
        A: PageFrameAllocator,
    {
        let range = PageFrame::range(start, end);
        for frame in range {
            self.map_identity(frame, flags, alloc, slot_alloc, temp_mapper);
        }
    }

    /// Maps the page to an unused page frame
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be mapped
    /// - `flags` the page frame for the page to be mapped on to
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map<A>(
        &mut self,
        page: Page,
        flags: EntryFlags,
        alloc: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
    ) where
        A: PageFrameAllocator,
    {
        let frame = alloc.falloc().expect("Out of memory");
        self.map_to(page, frame, flags, alloc, slot_alloc, temp_mapper);
    }

    /// Maps the specified page to the specified page frame
    /// using the provided flags.
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be mapped
    /// - `frame` the page frame for the page to be mapped on to
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    ///
    /// ## Returns
    /// The PML1 mapper used to map the page
    pub fn map_to<A>(
        &mut self,
        page: Page,
        frame: PageFrame,
        flags: EntryFlags,
        alloc: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
    ) -> PageTableMappingChain
    where
        A: PageFrameAllocator,
    {
        let is_phys = self.is_phys_identity();

        let pml4_index = page.p4_index();
        let mut pml3 = self.next_table_create(pml4_index, is_phys, alloc, slot_alloc, temp_mapper);

        let pml3_index = page.p3_index();
        let mut pml2 = pml3.next_table_create(pml3_index, is_phys, alloc, slot_alloc, temp_mapper);

        let pml2_index = page.p2_index();
        let mut pml1 = pml2.next_table_create(pml2_index, is_phys, alloc, slot_alloc, temp_mapper);

        let pml1_index = page.p1_index();
        pml1.set(pml1_index, frame, flags | EntryFlags::PRESENT);

        PageTableMappingChain {
            pml3: pml3,
            pml2: pml2,
            pml1: pml1
        }
    }

    /// Removes the page mapping, frees all frames contained
    /// in the page
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be unmapped
    /// - `temp_mapper` a reference to the 
    /// global temporary page mapping manager
    pub fn unmap(&mut self, page: Page, temp_mapper: &mut TempMapper) -> Option<()> {
        let p4_index = page.p4_index();
        let pml3 = self.next_table_temp(p4_index, temp_mapper)?;

        let p3_index = page.p3_index();
        let pml2 = pml3.next_table_temp(p3_index, temp_mapper)?;

        let p2_index = page.p2_index();
        let mut pml1 = pml2.next_table_temp(p2_index, temp_mapper)?;

        let p1_index = page.p1_index();
        let entry = &mut pml1.entries_mut()[p1_index];
        entry.set_to_unused();

        Some(())
    }

    /// Checks whether a page has already been mapped.
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be checked
    /// - `temp_mapper` a reference to the
    /// global temporary page mapping manager
    pub fn is_unused(&mut self, page: Page, temp_mapper: &mut TempMapper) -> bool {
        let is_mapped = (|| -> Option<()> {
            let pml3 = self.next_table_temp(page.p4_index(), temp_mapper)?;
            let pml2 = pml3.next_table_temp(page.p3_index(), temp_mapper)?;
            let pml1 = pml2.next_table_temp(page.p2_index(), temp_mapper)?;

            let entry = &pml1.entries()[page.p1_index()];
            (!entry.is_unused()).then_some(())
        })()
        .is_some();

        !is_mapped
    }

    /// Translates a virtual address to a physical one.
    ///
    /// ## Arguments
    ///
    /// - `addr` the virtual address to be mapped,
    /// assuming it's aligned to 0x1000
    /// - `temp_mapper` a reference to the global
    /// temporary page mapping manager
    pub fn translate_to_phys(
        &mut self,
        addr: usize,
        temp_mapper: &mut TempMapper,
    ) -> Option<PageFrame> {
        // need to walk the page table
        let page = Page::for_address(addr);

        let p4_index = page.p4_index();
        let pml3 = self.next_table_temp(p4_index, temp_mapper)?;

        let p3_index = page.p3_index();
        let pml2 = pml3.next_table_temp(p3_index, temp_mapper)?;

        let p2_index = page.p2_index();
        let pml1 = pml2.next_table_temp(p2_index, temp_mapper)?;

        let p1_index = page.p1_index();
        pml1.get_frame(p1_index)
    }

    /// Creates a temporary next-level page table. Only valid until the temporary page
    /// mapping changes, only use when in control of temporary page state!
    ///
    /// ## Arguments
    ///
    /// - `index` the index of the current table to get the next table from
    /// - `temp_mapper` a reference to the global temporary page mapping manager
    pub fn next_table_temp(&self, index: usize, temp_mapper: &mut TempMapper) -> Option<PageTable> {
        let entries = self.entries();
        let entry = &entries[index];

        let unused = entry.is_unused();
        if unused {
            None
        } else {
            let frame = entry.get_frame()?;
            let table_temp_addr = temp_mapper.set(frame);
            Some(PageTable::new(table_temp_addr))
        }
    }

    pub fn set(&mut self, index: usize, frame: PageFrame, flags: EntryFlags) {
        let mut entries = self.entries_mut();
        let entry = &mut entries[index];
        entry.set(frame, flags);
    }

    pub fn get_frame(&self, index: usize) -> Option<PageFrame> {
        let entries = self.entries();
        let entry = &entries[index];
        entry.get_frame()
    }

    /// Determines whether the page table is mapped referencing
    /// a virtual address page table slot, or an identity-mapped
    /// physical frame.
    pub fn is_phys_identity(&self) -> bool {
        self.addr < PAGE_TABLE_REGION_START
    }

    fn entries_mut(&mut self) -> &'static mut PageTableEntries {
        let pml4 = self.addr as *mut PageTableEntries;
        unsafe { &mut *pml4 }
    }

    fn entries(&self) -> &'static PageTableEntries {
        let pml4 = self.addr as *const PageTableEntries;
        let pml4 = unsafe { &*pml4 };
        return &pml4;
    }

    pub fn next_table_create<A>(
        &mut self,
        index: usize,
        return_physical: bool,
        pf_alloc: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
    ) -> PageTable
    where
        A: PageFrameAllocator,
    {
        let entries = &mut self.entries_mut();
        let entry = &mut entries[index];

        if !entry.is_unused() {
            if return_physical {
                // return a page table referencing the
                // physical address defined in the entry
                let addr = entry.get_frame().unwrap().start_address();
                PageTable::new(addr)
            } else {
                // create temporary next table mapping
                self.next_table_temp(index, temp_mapper).unwrap()
            }
        } else {
            // allocate new mapped table
            let slot = slot_alloc.alloc(pf_alloc, temp_mapper).unwrap();

            // the following contains a call to `translate_to_phys`,
            // which overrides the temporary mapping, thus we need
            // to save it and restore it later, since the current
            // table might be temp-mapped.
            let mut temp_mapping_restore: Option<PageFrame> = None;

            let slot_phys = if self.is_phys_identity() {
                // if we're still in initial identity mapping system,
                // the slot allocator returns physical addresses
                PageFrame::from_address(slot)
            } else {
                // set temp_mapping_restore since `translate_to_phys`
                // overrides the temporary mapping
                let current_phys = temp_mapper.get_current_phys();
                temp_mapping_restore = current_phys;

                let mut pml4 = slot_alloc.get_pml4();
                pml4.translate_to_phys(slot, temp_mapper).unwrap()
            };

            if let Some(restore_phys) = temp_mapping_restore {
                // restore temp_mapping_restore
                temp_mapper.set(restore_phys);
            }

            // set entry
            let flags = EntryFlags::PRESENT | EntryFlags::WRITABLE;
            entry.set(slot_phys, flags);

            PageTable::new(slot)
        }
    }
}

type PageTableEntries = [PageTableEntry; 512];

pub struct PageTableMappingChain {
    pub pml3: PageTable,
    pub pml2: PageTable,
    pub pml1: PageTable,
}