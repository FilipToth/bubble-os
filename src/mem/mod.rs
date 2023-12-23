mod simple_page_frame_allocator;

pub use self::simple_page_frame_allocator::SimplePageFrameAllocator;

pub static FRAME_SIZE: usize = 4096;

#[derive(Debug)]
pub struct PageFrame {
    pub frame_number: usize
}

impl PageFrame {
    fn from_address(addr: usize) -> PageFrame {
        let number = addr / FRAME_SIZE;
        PageFrame { frame_number: number }
    }

    pub fn get_address(&self) -> usize {
        self.frame_number * FRAME_SIZE
    }

    fn clone(&self) -> PageFrame {
        PageFrame { frame_number: self.frame_number }
    }
}

pub trait PageFrameAllocator {
    fn falloc(&mut self) -> Option<PageFrame>;
    fn free(&mut self);
}
