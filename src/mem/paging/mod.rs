use core::ptr::Unique;

use x86_64::VirtAddr;
use x86_64::instructions::tlb;

use crate::mem::PageFrameAllocator;
use crate::mem::{PAGE_SIZE, PageFrame};
use crate::mem::paging::entry::EntryFlags;
use crate::mem::{PhysicalAddress, VirtualAddress};

pub mod entry;
pub mod temp_page;
pub mod page_table;

use self::page_table::{P4, PageLevel4};
use self::page_table::PageTable;
use self::temp_page::TempPage;

const TABLE_ENTRY_COUNT: usize = 512;

#[derive(Debug, Clone, Copy)]
pub struct Page {
    page_number: usize
}

impl Page {
    /// Instantiates a new unmapped page to for corresponding
    /// virtual address.
    /// 
    /// # Arguments
    /// 
    /// - `addr` the virtual address for the page to be
    /// mapped on to
    pub fn for_address(addr: VirtualAddress) -> Page {
        assert!(addr < 0x0000_8000_0000_0000 || addr >= 0xFFFF_8000_0000_0000);
        Page { page_number: addr / PAGE_SIZE }
    }

    fn start_address(&self) -> VirtualAddress {
        self.page_number * PAGE_SIZE
    }

    fn p4_index(&self) -> usize {
        (self.page_number >> 27) & 0o777
    }

    fn p3_index(&self) -> usize {
        (self.page_number >> 18) & 0o777
    }

    fn p2_index(&self) -> usize {
        (self.page_number >> 9) & 0o777
    }

    fn p1_index(&self) -> usize {
        (self.page_number >> 0) & 0o777
    }
}

/// A page table that is currently loaded in the CPU
pub struct ActivePageTable {
    p4: Unique<PageTable<PageLevel4>>
}

impl ActivePageTable {
    pub unsafe fn new() -> ActivePageTable {
        ActivePageTable {
            p4: Unique::new_unchecked(P4)
        }
    }

    fn get_p4(&self) -> &PageTable<PageLevel4> {
        unsafe {
            self.p4.as_ref()
        }
    }

    fn get_p4_mut(&mut self) -> &mut PageTable<PageLevel4> {
        unsafe {
            self.p4.as_mut()
        }
    }

    /// Maps the specified page to the specified page frame
    /// using the provided flags.
    /// 
    /// # Arguments
    /// 
    /// - `page` the page to be mapped
    /// - `frame` the page frame for the page to be mapped on to
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map_to<A>(&mut self, page: Page, frame: PageFrame, flags: EntryFlags, allocator: &mut A)
        where A: PageFrameAllocator
    {
        let p4 = self.get_p4_mut();
        let p3 = p4.next_table_create(page.p4_index(), allocator);
        let p2 = p3.next_table_create(page.p3_index(), allocator);
        let p1 = p2.next_table_create(page.p2_index(), allocator);

        let entry = &mut p1[page.p1_index()];
        assert!(entry.is_unused());

        entry.set(frame, flags | EntryFlags::PRESENT);
    }

    /// Maps the page to an unused page frame
    /// 
    /// # Arguments
    /// 
    /// - `page` the page to be mapped
    /// - `flags` the page frame for the page to be mapped on to
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map<A>(&mut self, page: Page, flags: EntryFlags, allocator: &mut A)
        where A: PageFrameAllocator
    {
        let frame = allocator.falloc().expect("Out of memory");
        self.map_to(page, frame, flags, allocator);
    }

    /// Maps a page to its exact corresponding page frame
    /// 
    /// # Arguments
    /// 
    /// - `frame` the page frame for the page to be mapped on to
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables 
    pub fn map_identity<A>(&mut self, frame: PageFrame, flags: EntryFlags, allocator: &mut A)
        where A: PageFrameAllocator
    {
        let page = Page::for_address(frame.start_address());
        self.map_to(page, frame, flags, allocator);
    }
    
    /// Removes the page mapping, frees all frames contained
    /// in the page
    /// 
    /// # Arguments
    /// 
    /// - `page` the page to be unmapped
    /// - `allocator` the page frame allocator to perform
    /// the freeing of the page frames
    pub fn unmap<A>(&mut self, page: Page, allocator: &mut A)
        where A: PageFrameAllocator
    {
        assert!(self.translate_to_phys(page.start_address()).is_some());

        let p1 = self.get_p4_mut()
                     .next_table_mut(page.p4_index())
                     .and_then(|p3| p3.next_table_mut(page.p3_index()))
                     .and_then(|p2| p2.next_table_mut(page.p2_index()))
                     .expect("Mapping code doesn't support huge pages");

        // we also need to flush the TLB cache
        // manually, if we don't do this, reading
        // out of pages would still be possible
        // after unmapping due to them still
        // being in the TLB cache.

        let entry = &mut p1[page.p1_index()];
        let virt_addr = VirtAddr::new(page.start_address() as u64);

        entry.set_to_unused();
        tlb::flush(virt_addr)
        
        // We could also free the tables once all pages are empty...
        // TODO: Implement allocator.free   

        // let frame = entry.get_frame().unwrap();
        // allocator.free(frame);
    }

    /// Translates a virtual address to a physical one.
    /// 
    /// # Arguments
    /// 
    /// - `addr` the virtual address to be mapped
    ///
    pub fn translate_to_phys(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
        let offset = addr % PAGE_SIZE;
        self.translate_page(Page::for_address(addr))
            .map(|frame| frame.frame_number * PAGE_SIZE + offset)
    }
    
    fn translate_page(&self, page: Page) -> Option<PageFrame> {
        let p3 = self.get_p4().next_table(page.p4_index());
    
        let huge_page = || {
            p3.and_then(|p3| {
                let p3_entry = &p3[page.p3_index()];
    
                // is it a 1 GiB page?
                if let Some(start_frame) = p3_entry.get_frame() {
                    if p3_entry.flags().contains(EntryFlags::HUGE_PAGE) {
                        // addr must be aligned to 1 GiB
                        let gb_align = TABLE_ENTRY_COUNT ^ 2;
                        assert!(start_frame.frame_number % gb_align == 0);
    
                        let num = start_frame.frame_number + (page.p2_index() * TABLE_ENTRY_COUNT) + page.p1_index();
                        let frame = PageFrame {
                            frame_number: num
                        };
    
                        return Some(frame);
                    }
                }
    
                if let Some(p2) = p3.next_table(page.p3_index()) {
                    let p2_entry = &p2[page.p2_index()];
    
                    // is it a 2 MiB page?
                    if let Some(start_frame) = p2_entry.get_frame() {
                        if p2_entry.flags().contains(EntryFlags::HUGE_PAGE) {
                            // must be aligned to 2 MiB
                            assert!(start_frame.frame_number % TABLE_ENTRY_COUNT == 0);
    
                            let num = start_frame.frame_number + page.p1_index();
                            let frame = PageFrame {
                                frame_number: num
                            };
    
                            return Some(frame);
                        }
                    }
                }
    
                None
            })
        };
    
        p3.and_then(|p3| p3.next_table(page.p3_index()))
          .and_then(|p2| p2.next_table(page.p2_index()))
          .and_then(|p1| p1[page.p1_index()].get_frame())
          .or_else(huge_page)
    }
}

/// A page table which isn't loaded in the CPU.
pub struct InactivePageTable {
    p4_frame: PageFrame
}

impl InactivePageTable {
    /// Instantiates an inactive page table
    /// for a frame to be used for the p4.
    /// 
    /// # Arguments
    /// 
    /// - `frame` the frame to be used for the p4
    pub fn new(frame: PageFrame, active_table: &mut ActivePageTable, temp_page: &mut TempPage) -> InactivePageTable {
        // we need to null the frame, but the frame
        // isn't yet mapped to a virtual address,
        // therefore we need to create a temporary
        // mapping.
        
        {
            let table = temp_page.map_table_frame(frame.clone(), active_table);

            table.null_all_entries();
            table[511].set(frame.clone(), EntryFlags::PRESENT | EntryFlags::WRITABLE);

            // drop the table
        }

        temp_page.unmap(active_table);
        InactivePageTable { p4_frame: frame }
    }
}
