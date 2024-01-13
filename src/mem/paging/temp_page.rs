use core::panic;

use crate::mem::PageFrameAllocator;
use crate::mem::paging::entry::EntryFlags;
use crate::mem::{PageFrame, VirtualAddress};
use crate::mem::paging::{ActivePageTable, Page};
use crate::mem::paging::page_table::{PageTable, PageLevel1};


pub struct TempPage {
    page: Page,
    allocator: FixedAllocator
}

impl TempPage {
    /// Instantiates a new temporary page
    /// 
    /// # Arguments
    /// 
    /// - `page` page structure to make temporary
    /// - `allocator` needs a page frame allocator to
    /// create page tables for mappings
    pub fn new<A>(page: Page, allocator: &mut A) -> TempPage
        where A: PageFrameAllocator
    {
        let fixed = FixedAllocator::new(allocator);
        TempPage {
            page: page,
            allocator: fixed
        }
    }

    /// Creates a mapping for the temporary page
    /// to the supplied frame using the active
    /// page table.
    /// 
    /// # Arguments
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
        let already_mapped = active_table.translate_to_phys(self.page.start_address()).is_none();
        assert!(already_mapped, "Temp page is already mapped");

        active_table.map_to(self.page, frame, EntryFlags::WRITABLE, &mut self.allocator);
        self.page.start_address()
    }
    
    /// Unmaps the temporary page from the
    /// active page table.
    /// 
    /// # Arguments
    /// 
    /// - `active_table` the active page table that
    /// should perform the mapping
    pub fn unmap(&mut self, active_table: &mut ActivePageTable) {
        active_table.unmap(self.page, &mut self.allocator);
    }

    ///
    pub fn map_table_frame(&mut self, frame: PageFrame, active_table: &mut ActivePageTable) -> &mut PageTable<PageLevel1> {
        let addr = self.map(frame, active_table);
        unsafe {
            &mut *(addr as *mut PageTable<PageLevel1>)
        }
    }
}

struct FixedAllocator {
    alloc_pool: [Option<PageFrame>; 3]
}

impl FixedAllocator {
    fn new<A>(allocator: &mut A) -> FixedAllocator
        where A: PageFrameAllocator
    {
        let mut f = || allocator.falloc();
        let pool = [f(), f(), f()];

        FixedAllocator { alloc_pool: pool }
    }
}

impl PageFrameAllocator for FixedAllocator {
    fn falloc(&mut self) -> Option<PageFrame> {
        for frame in &mut self.alloc_pool {
            match frame {
                Some(_) => return frame.take(),
                None => continue
            }
        }

        None
    }

    fn free(&mut self, frame: PageFrame) {
        for owned_frame in &mut self.alloc_pool {
            if owned_frame.is_none() {
                *owned_frame = Some(frame);
                return;
            }
        }

        panic!("Fixed allocator cannot free another frame.");
    }
}
