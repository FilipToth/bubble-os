use crate::mem::paging::{
    slot_allocator::PageTableSlotAllocator, temp_mapper::TempMapper, PageIter,
};

use alloc::vec::Vec;

use super::{
    paging::{entry::EntryFlags, Page, PageTable},
    stack::Stack,
    PageFrameAllocator, PAGE_SIZE,
};

struct FreeStackRange {
    start: Page,
    end: Page,
    user_pages: usize,
}

pub struct StackAllocator {
    range: PageIter,
    free_ranges: Vec<FreeStackRange>,
}

impl StackAllocator {
    pub fn new(range: PageIter) -> StackAllocator {
        StackAllocator {
            range: range,
            free_ranges: Vec::new(),
        }
    }

    pub fn alloc<A: PageFrameAllocator>(
        &mut self,
        table: &mut PageTable,
        frame_allocator: &mut A,
        slot_alloc: &mut PageTableSlotAllocator,
        temp_mapper: &mut TempMapper,
        pages_to_alloc: usize,
        flags: EntryFlags,
    ) -> Option<Stack> {
        if pages_to_alloc == 0 {
            return None;
        }

        if let Some(index) = self
            .free_ranges
            .iter()
            .position(|range| range.user_pages == pages_to_alloc)
        {
            let range = self.free_ranges.remove(index);
            for page in Page::range(range.start, range.end) {
                table.map(page, flags, frame_allocator, slot_alloc, temp_mapper);
            }

            let stack_top = range.end.start_address() + PAGE_SIZE;
            return Some(Stack::new(stack_top, range.start.start_address()));
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
                    table.map(page, flags, frame_allocator, slot_alloc, temp_mapper);
                }

                let stack_top = end.start_address() + PAGE_SIZE;
                let stack = Stack::new(stack_top, start.start_address());
                Some(stack)
            }
            _ => None,
        }
    }

    pub fn free(&mut self, stack: &Stack) {
        if stack.bottom < PAGE_SIZE || stack.top <= stack.bottom {
            return;
        }

        let start = Page::for_address(stack.bottom);
        let end = Page::for_address(stack.top - 1);
        let user_pages = (stack.top - stack.bottom) / PAGE_SIZE;

        self.free_ranges.push(FreeStackRange {
            start: start,
            end: end,
            user_pages: user_pages,
        });
    }
}
