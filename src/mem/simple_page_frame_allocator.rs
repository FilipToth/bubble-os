use super::{PageFrame, PageFrameAllocator, PAGE_SIZE};

pub struct SimplePageFrameAllocator {
    frame_head: PageFrame,
    // mem_start: PageFrame,
    mem_end: PageFrame,
}

impl SimplePageFrameAllocator {
    pub fn new(mem_start: usize, mem_end: usize) -> SimplePageFrameAllocator {
        let start_frame = PageFrame::from_address(mem_start);
        let end_frame = PageFrame::from_address(mem_end);

        SimplePageFrameAllocator {
            frame_head: start_frame,
            // mem_start: start_frame,
            mem_end: end_frame,
        }
    }
}

impl PageFrameAllocator for SimplePageFrameAllocator {
    fn falloc(&mut self) -> Option<PageFrame> {
        let end_addr = self.mem_end.start_address();
        let head_addr = self.frame_head.start_address();

        if end_addr - (head_addr + PAGE_SIZE) < 4096 {
            return None;
        }

        let next_frame = PageFrame::from_address(head_addr + PAGE_SIZE);
        self.frame_head = next_frame.clone();
        Some(next_frame)
    }

    fn free(&mut self, _frame: PageFrame) {
        todo!()
    }
}
