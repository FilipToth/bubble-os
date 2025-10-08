use crate::mem::{paging::{entry::{EntryFlags, PageTableEntry}, Page}, PageFrame, PageFrameAllocator};

pub struct PageTable {
    frame: PageFrame
}

impl PageTable {
    pub fn new(frame: PageFrame) -> Self {
        Self { frame: frame }
    }

    pub fn map<A>(&mut self, page: Page, flags: EntryFlags, alloc: &mut A)
    where
        A: PageFrameAllocator
    {
        unreachable!()
    }

    pub fn unmap<A>(&mut self, page: Page, alloc: &mut A)
    where
        A: PageFrameAllocator
    {
        unreachable!()
    }

    pub fn map_identity<A>(&mut self, frame: PageFrame, flags: EntryFlags, alloc: &mut A)
    where
        A: PageFrameAllocator
    {
        unreachable!()
    }

    pub fn map_to<A>(&mut self, page: Page, frame: PageFrame, flags: EntryFlags, alloc: &mut A)
    where
        A: PageFrameAllocator
    {
        unreachable!()
    }

    pub fn is_unused<A>(&mut self, page: Page, alloc: &mut A) -> bool
    where
        A: PageFrameAllocator
    {
        unreachable!()
    }

    pub fn translate_to_phys(&mut self, addr: usize) -> Option<usize> {
        unreachable!()
    }

    fn entries_mut(&mut self) -> &'static mut PageTableEntries {
        let pml4 = self.frame.start_address() as *mut PageTableEntries;
        unsafe { &mut *pml4 }
    }

    fn entries(&self) -> &'static PageTableEntries {
        let pml4 = self.frame.start_address() as *const PageTableEntries;
        let pml4 = unsafe { &*pml4 };
        return &pml4;
    }
}

type PageTableEntries = [PageTableEntry; 512];
