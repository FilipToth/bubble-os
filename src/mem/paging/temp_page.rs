use core::panic;

use crate::mem::paging::entry::EntryFlags;
use crate::mem::paging::page_table::{PageLevel1, PageTable};
use crate::mem::paging::{ActivePageTable, Page};
use crate::mem::PageFrameAllocator;
use crate::mem::{PageFrame, VirtualAddress};

pub struct TempPage {
    page: Page,
    allocator: FixedAllocator,
}

impl TempPage {
    /// Instantiates a new temporary page
    ///
    /// ## Arguments
    ///
    /// - `page` page structure to make temporary
    /// - `allocator` needs a page frame allocator to
    /// create page tables for mappings
    pub fn new<A>(page: Page, allocator: &mut A) -> Option<TempPage>
    where
        A: PageFrameAllocator,
    {
        let fixed = FixedAllocator::new(allocator);
        Some(TempPage {
            page: page,
            allocator: fixed,
        })
    }

    /// Creates a mapping for the temporary page
    /// to the supplied frame using the active
    /// page table.
    ///
    /// ## Arguments
    ///
    /// - `frame` the frame for the page to be mapped
    /// on to.
    /// - `active_table` the active page table that
    /// should perform the mapping
    ///
    /// # Returns
    ///
    /// The start virtual address of the temporary
    /// page
    pub fn map(&mut self, frame: PageFrame, active_table: &mut ActivePageTable) -> VirtualAddress {
        let already_mapped = active_table
            .translate_to_phys(self.page.start_address())
            .is_none();

        assert!(already_mapped, "Temp page is already mapped");

        active_table.map_to(self.page, frame, EntryFlags::WRITABLE, &mut self.allocator);
        self.page.start_address()
    }

    /// Unmaps the temporary page from the
    /// active page table.
    ///
    /// ## Arguments
    ///
    /// - `active_table` the active page table that
    /// should perform the mapping
    pub fn unmap(&mut self, active_table: &mut ActivePageTable) {
        active_table.unmap(self.page, &mut self.allocator);
    }

    pub fn map_table_frame(
        &mut self,
        frame: PageFrame,
        active_table: &mut ActivePageTable,
    ) -> &mut PageTable<PageLevel1> {
        let addr = self.map(frame, active_table);
        unsafe { &mut *(addr as *mut PageTable<PageLevel1>) }
    }
}

struct FixedAllocator {
    // arrays don't work for some reason
    frame_1: Option<PageFrame>,
    frame_2: Option<PageFrame>,
    frame_3: Option<PageFrame>,
}

impl FixedAllocator {
    fn new<A>(allocator: &mut A) -> FixedAllocator
    where
        A: PageFrameAllocator,
    {
        let mut f = || allocator.falloc();
        FixedAllocator {
            frame_1: f(),
            frame_2: f(),
            frame_3: f(),
        }
    }
}

impl PageFrameAllocator for FixedAllocator {
    fn falloc(&mut self) -> Option<PageFrame> {
        // I know, it's ugly, but
        // arrays didn't work
        if self.frame_1.is_some() {
            self.frame_1.take()
        } else if self.frame_2.is_some() {
            self.frame_2.take()
        } else if self.frame_3.is_some() {
            self.frame_3.take()
        } else {
            None
        }
    }

    fn free(&mut self, frame: PageFrame) {
        if self.frame_1.is_none() {
            self.frame_1 = Some(frame)
        } else if self.frame_2.is_none() {
            self.frame_2 = Some(frame)
        } else if self.frame_3.is_none() {
            self.frame_3 = Some(frame)
        } else {
            panic!("Fixed allocator cannot free another frame")
        }
    }
}
