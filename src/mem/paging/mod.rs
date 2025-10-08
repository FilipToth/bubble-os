use core::ops::Add;

use crate::mem::{PageFrameAllocator, VirtualAddress, PAGE_SIZE};
pub use page_table::PageTable;

pub mod entry;
mod page_table;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Page {
    page_number: usize,
}

impl Page {
    /// Instantiates a new unmapped page to for corresponding
    /// virtual address.
    ///
    /// ## Arguments
    ///
    /// - `addr` the virtual address for the page to be
    /// mapped on to
    pub fn for_address(addr: VirtualAddress) -> Page {
        assert!(addr < 0x0000_8000_0000_0000 || addr >= 0xFFFF_8000_0000_0000);
        Page {
            page_number: addr / PAGE_SIZE,
        }
    }

    pub fn start_address(&self) -> VirtualAddress {
        self.page_number * PAGE_SIZE
    }

    // TODO: This is bullshit and relies on previous recursive indexing mechanism
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

    pub fn range(start: Page, end: Page) -> PageIter {
        PageIter {
            start: start,
            end: end,
        }
    }
}

impl Add<usize> for Page {
    type Output = Page;

    fn add(self, rhs: usize) -> Self::Output {
        Page {
            page_number: self.page_number + rhs,
        }
    }
}

#[derive(Clone)]
pub struct PageIter {
    start: Page,
    end: Page,
}

impl Iterator for PageIter {
    type Item = Page;

    fn next(&mut self) -> Option<Page> {
        if self.start > self.end {
            return None;
        }

        let frame = self.start.clone();
        self.start.page_number += 1;
        Some(frame)
    }
}

pub fn init_new_pml4<A>(allocator: &mut A) -> PageTable
where
    A: PageFrameAllocator
{
    let pml4_frame = allocator.falloc().unwrap();
    let pml4 = PageTable::new(pml4_frame);
    return pml4;
}