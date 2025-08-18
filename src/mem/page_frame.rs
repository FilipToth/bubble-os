use super::PhysicalAddress;

pub static PAGE_SIZE: usize = 4096;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct PageFrame {
    pub frame_number: usize,
}

impl PageFrame {
    pub fn from_address(addr: usize) -> PageFrame {
        let number = addr / PAGE_SIZE;
        PageFrame {
            frame_number: number,
        }
    }

    pub fn start_address(&self) -> PhysicalAddress {
        self.frame_number * PAGE_SIZE
    }

    pub fn range(start: PageFrame, end: PageFrame) -> PageFrameIter {
        PageFrameIter {
            start: start,
            end: end,
        }
    }

    pub fn clone(&self) -> PageFrame {
        PageFrame {
            frame_number: self.frame_number,
        }
    }
}

pub struct PageFrameIter {
    start: PageFrame,
    end: PageFrame,
}

impl Iterator for PageFrameIter {
    type Item = PageFrame;

    fn next(&mut self) -> Option<PageFrame> {
        if self.start > self.end {
            return None;
        }

        let frame = self.start.clone();
        self.start.frame_number += 1;
        Some(frame)
    }
}

pub trait PageFrameAllocator {
    fn falloc(&mut self) -> Option<PageFrame>;
    fn free(&mut self, frame: PageFrame);
}
