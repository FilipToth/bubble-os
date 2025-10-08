use crate::mem::paging::PageIter;

use super::{
    paging::{entry::EntryFlags, PageTable, Page},
    stack::Stack,
    PageFrameAllocator, PAGE_SIZE,
};

pub struct StackAllocator {
    range: PageIter,
}

impl StackAllocator {
    pub fn new(range: PageIter) -> StackAllocator {
        StackAllocator { range: range }
    }

    pub fn alloc<A: PageFrameAllocator>(
        &mut self,
        active_table: &mut PageTable,
        frame_allocator: &mut A,
        pages_to_alloc: usize,
        flags: EntryFlags,
    ) -> Option<Stack> {
        if pages_to_alloc == 0 {
            return None;
        }

        let mut range = self.range.clone();
        let guard = range.next();
        let start = range.next();

        let end = if pages_to_alloc == 1 {
            start
        } else {
            // index starts at 0 and we've already
            // allocated the start page
            range.nth(pages_to_alloc - 2)
        };

        match (guard, start, end) {
            (Some(_), Some(start), Some(end)) => {
                // update range to also include guard page
                self.range = range;

                for page in Page::range(start, end) {
                    active_table.map(page, flags, frame_allocator);
                }

                let stack_top = end.start_address() + PAGE_SIZE;
                let stack = Stack::new(stack_top, start.start_address());
                Some(stack)
            }
            _ => None,
        }
    }
}
