pub mod paging;
mod linked_list_allocator;
mod simple_page_frame_allocator;

pub use self::simple_page_frame_allocator::SimplePageFrameAllocator;

pub type VirtualAddress = usize;
pub type PhysicalAddress = usize;

pub static PAGE_SIZE: usize = 4096;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageFrame {
    pub frame_number: usize
}

impl PageFrame {
    fn from_address(addr: usize) -> PageFrame {
        let number = addr / PAGE_SIZE;
        PageFrame { frame_number: number }
    }

    pub fn start_address(&self) -> PhysicalAddress {
        self.frame_number * PAGE_SIZE
    }

    fn clone(&self) -> PageFrame {
        PageFrame { frame_number: self.frame_number }
    }

    fn range(start: PageFrame, end: PageFrame) -> PageFrameIter {
        PageFrameIter { start: start, end: end }
    }
}

struct PageFrameIter {
    start: PageFrame,
    end: PageFrame
}

impl Iterator for PageFrameIter {
    type Item = PageFrame;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start > self.end {
            return None;
        }
        
        let current = self.start.clone();
        current.frame_number += 1;
        Some(current)
    }
}

pub trait PageFrameAllocator {
    fn falloc(&mut self) -> Option<PageFrame>;
    fn free(&mut self, frame: PageFrame);
}
