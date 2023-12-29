use core::ptr::Unique;

use crate::mem::PageFrameAllocator;
use crate::mem::{PAGE_SIZE, PageFrame};
use crate::mem::paging::entry::EntryFlags;
use crate::mem::{PhysicalAddress, VirtualAddress};

pub mod entry;
pub mod page_table;

use self::page_table::{P4, PageLevel4};
use self::page_table::PageTable;

const TABLE_ENTRY_COUNT: usize = 512;

pub struct Page {
    page_number: usize
}

impl Page {
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

pub struct ActivePageTable {
    p4: Unique<PageTable<PageLevel4>>
}

impl ActivePageTable {
    pub fn map_to<A>(page: Page, frame: PageFrame, flags: EntryFlags, allocator: &mut A)
        where A: PageFrameAllocator
    {
        let p4 = unsafe {
            &mut *P4
        };

        let mut p3 = p4.next_table_create(page.p4_index(), allocator);
        let mut p2 = p3.next_table_create(page.p3_index(), allocator);
        let mut p1 = p2.next_table_create(page.p2_index(), allocator);

        assert!(p1[page.p1_index()].is_unused());

        let entry = &mut p1[page.p1_index()];
        entry.set(frame, flags | EntryFlags::PRESENT);
    }
}

pub fn translate_to_phys(addr: VirtualAddress) -> Option<PhysicalAddress> {
    let offset = addr % PAGE_SIZE;
    translate_page(Page::for_address(addr))
        .map(|frame| frame.frame_number * PAGE_SIZE + offset)
}

fn translate_page(page: Page) -> Option<PageFrame> {
    let p3 = unsafe {
        &*page_table::P4
    }.next_table(page.p4_index());

    let huge_page = || {
        p3.and_then(|p3| {
            let p3_entry = &p3[page.p3_index()];

            // is it a 1 GB page?
            if let Some(start_frame) = p3_entry.get_frame() {
                if p3_entry.flags().contains(EntryFlags::HUGE_PAGE) {
                    // addr must be aligned to 1 GB
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

                // is it a 2 MB page?
                if let Some(start_frame) = p2_entry.get_frame() {
                    if p2_entry.flags().contains(EntryFlags::HUGE_PAGE) {
                        // must be aligned to 2 MB
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