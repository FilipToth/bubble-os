use core::ptr::Unique;

use x86_64::instructions::tlb;
use x86_64::VirtAddr;

use crate::mem::paging::entry::EntryFlags;
use crate::mem::paging::page_table::{PageLevel3, PageLevel4, PageTable, P4};
use crate::mem::PageFrameAllocator;
use crate::mem::{PageFrame, PAGE_SIZE};
use crate::mem::{PhysicalAddress, VirtualAddress};

use crate::mem::paging::{Page, TABLE_ENTRY_COUNT};
use crate::print;

pub struct Mapper {
    p4: Unique<PageTable<PageLevel4>>,
}

impl Mapper {
    pub unsafe fn new() -> Mapper {
        Mapper {
            p4: Unique::new_unchecked(P4),
        }
    }

    pub fn get_p4(&self) -> &PageTable<PageLevel4> {
        unsafe { self.p4.as_ref() }
    }

    pub fn get_p4_mut(&mut self) -> &mut PageTable<PageLevel4> {
        unsafe { self.p4.as_mut() }
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
    pub fn map_to<A>(&mut self, page: Page, frame: PageFrame, flags: EntryFlags, allocator: &mut A)
    where
        A: PageFrameAllocator,
    {
        let p4 = self.get_p4_mut();
        let p3 = p4.next_table_create(page.p4_index(), allocator);
        let p2 = p3.next_table_create(page.p3_index(), allocator);
        let p1 = p2.next_table_create(page.p2_index(), allocator);

        let entry = &mut p1[page.p1_index()];
        assert!(entry.is_unused());

        entry.set(frame, flags | EntryFlags::PRESENT);
    }

    pub fn verify<A>(&mut self, page: Page, allocator: &mut A)
    where
        A: PageFrameAllocator
    {
        let p4 = self.get_p4_mut();
        let p4e = &p4[page.p4_index()];
        let p4_flags = p4e.flags();
        print!("ELF P4e Flags: {:?}\n", p4_flags);

        let p3 = p4.next_table_create(page.p4_index(), allocator);
        let p3e = &mut p3[page.p3_index()];
        let p3_flags = p3e.flags();
        print!("ELF P3e Flags: {:?}\n", p3_flags);

        let p2 = p3.next_table_create(page.p3_index(), allocator);
        let p2e = &mut p2[page.p2_index()];
        let p2_flags = p2e.flags();
        print!("ELF P2e Flags: {:?}\n", p2_flags);

        let p1 = p2.next_table_create(page.p2_index(), allocator);
        let p1e = &mut p1[page.p1_index()];
        let p1_flags = p1e.flags();
        print!("ELF P1e Flags: {:?}\n", p1_flags);
    }

    /// Maps the page to an unused page frame
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be mapped
    /// - `flags` the page frame for the page to be mapped on to
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map<A>(&mut self, page: Page, flags: EntryFlags, allocator: &mut A)
    where
        A: PageFrameAllocator,
    {
        let frame = allocator.falloc().expect("Out of memory");
        self.map_to(page, frame, flags, allocator);
    }

    /// Maps a page to its exact corresponding page frame
    ///
    /// ## Arguments
    ///
    /// - `frame` the page frame for the page to be mapped on to
    /// - `flags` the page table entry flags to be used
    /// - `allocator` needs a page frame allocator to create
    /// page tables
    pub fn map_identity<A>(&mut self, frame: PageFrame, flags: EntryFlags, allocator: &mut A)
    where
        A: PageFrameAllocator,
    {
        let page = Page::for_address(frame.start_address());
        self.map_to(page, frame, flags, allocator);
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
        allocator: &mut A,
    ) where
        A: PageFrameAllocator,
    {
        let range = PageFrame::range(start, end);
        for frame in range {
            self.map_identity(frame, flags, allocator);
        }
    }

    /// Removes the page mapping, frees all frames contained
    /// in the page
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be unmapped
    /// - `allocator` the page frame allocator to perform
    /// the freeing of the page frames
    pub fn unmap<A>(&mut self, page: Page, allocator: &mut A)
    where
        A: PageFrameAllocator,
    {
        assert!(self.translate_to_phys(page.start_address()).is_some());
        let p1 = self
            .get_p4_mut()
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

    /// Checks whether a page has already been mapped.
    ///
    /// ## Arguments
    ///
    /// - `page` the page to be checked
    /// - `allocator` the page frame allocator used to
    /// get the page indices
    pub fn is_unused<A>(&mut self, page: Page, allocator: &mut A) -> bool
    where
        A: PageFrameAllocator,
    {
        let p4 = self.get_p4_mut();
        let p3 = p4.next_table_create(page.p4_index(), allocator);
        let p2 = p3.next_table_create(page.p3_index(), allocator);
        let p1 = p2.next_table_create(page.p2_index(), allocator);

        let entry = &mut p1[page.p1_index()];
        entry.is_unused()
    }

    /// Translates a virtual address to a physical one.
    ///
    /// ## Arguments
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

                        let num = start_frame.frame_number
                            + (page.p2_index() * TABLE_ENTRY_COUNT)
                            + page.p1_index();

                        let frame = PageFrame { frame_number: num };
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
                            let frame = PageFrame { frame_number: num };

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
