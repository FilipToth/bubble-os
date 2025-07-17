use core::marker::PhantomData;
use core::ops::{Index, IndexMut};

use crate::mem::paging::entry::*;
use crate::mem::paging::TABLE_ENTRY_COUNT;
use crate::mem::{PageFrame, PageFrameAllocator};
use crate::print;

pub const P4: *mut PageTable<PageLevel4> = 0xFFFFFFFF_FFFFF000 as *mut _;

pub struct PageTable<L: PageTableLevel> {
    pub entries: [PageTableEntry; TABLE_ENTRY_COUNT],
    level: PhantomData<L>,
}

impl<L> Index<usize> for PageTable<L>
where
    L: PageTableLevel,
{
    type Output = PageTableEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl<L> IndexMut<usize> for PageTable<L>
where
    L: PageTableLevel,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

impl<L> PageTable<L>
where
    L: PageTableLevel,
{
    pub fn null_all_entries(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.set_to_unused();
        }
    }

    pub fn list_entries(&self) {
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.is_unused() {
                continue;
            }

            print!(
                "New ({}) PML({:?}) entry: 0x{:X}, {:?}\n",
                index,
                self.level,
                entry.get_frame().unwrap().start_address(),
                entry.flags()
            );
        }
    }
}

impl PageTable<PageLevel4> {
    pub fn clone_pml4(&self, buffer: PageFrame) {
        let buffer_addr = buffer.start_address();
        let buffer_ptr = buffer_addr as *mut PageTable<PageLevel4>;

        let self_ptr = self as *const PageTable<PageLevel4>;
        unsafe { core::ptr::copy(self_ptr, buffer_ptr, 1) };
    }
}

impl<L> PageTable<L>
where
    L: HierarchicalPageLevel,
{
    pub fn next_table(&self, index: usize) -> Option<&PageTable<L::NextLevel>> {
        let addr = self.next_table_address(index);
        addr.map(|a| unsafe { &*(a as *const _) })
    }

    pub fn next_table_mut(&mut self, index: usize) -> Option<&mut PageTable<L::NextLevel>> {
        let addr = self.next_table_address(index);
        addr.map(|a| unsafe { &mut *(a as *mut _) })
    }

    fn next_table_address(&self, index: usize) -> Option<usize> {
        let flags = self[index].flags();
        if flags.contains(EntryFlags::PRESENT) && !flags.contains(EntryFlags::HUGE_PAGE) {
            let self_addr = (self as *const _) as usize;
            let next_addr = (self_addr << 9) | (index << 12);
            Some(next_addr)
        } else {
            None
        }
    }

    pub fn next_table_create<A>(
        &mut self,
        index: usize,
        allocator: &mut A,
    ) -> &mut PageTable<L::NextLevel>
    where
        A: PageFrameAllocator,
    {
        if self.next_table(index).is_none() {
            assert!(
                !self.entries[index].flags().contains(EntryFlags::HUGE_PAGE),
                "Mapping code doesn't support huge pages"
            );

            let frame = allocator.falloc().expect("No available frames to allocate");
            self.entries[index].set(frame, EntryFlags::PRESENT | EntryFlags::WRITABLE);

            self.next_table_mut(index).unwrap().null_all_entries();
        }

        self.next_table_mut(index).unwrap()
    }
}

impl PageTable<PageLevel4> {
    pub fn inspect(&self) {
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.is_unused() {
                continue;
            }

            print!(
                "PML4 entry addr: 0x{:X}\n",
                entry.get_frame().unwrap().start_address()
            );
            match self.next_table(index) {
                Some(pml3) => {
                    print!("\n");
                    for (index, entry) in pml3.entries.iter().enumerate() {
                        if entry.is_unused() {
                            continue;
                        }

                        print!(
                            "PML3: {}, entryflags: {:?}, entryaddr: 0x{:X}\n",
                            index,
                            entry.flags(),
                            entry.get_frame().unwrap().start_address()
                        );
                    }
                }
                None => continue,
            }
        }
    }
}

pub trait PageTableLevel {}

pub enum PageLevel4 {}
pub enum PageLevel3 {}
pub enum PageLevel2 {}
pub enum PageLevel1 {}

impl PageTableLevel for PageLevel4 {}
impl PageTableLevel for PageLevel3 {}
impl PageTableLevel for PageLevel2 {}
impl PageTableLevel for PageLevel1 {}

pub trait HierarchicalPageLevel: PageTableLevel {
    type NextLevel: PageTableLevel;
}

impl HierarchicalPageLevel for PageLevel4 {
    type NextLevel = PageLevel3;
}

impl HierarchicalPageLevel for PageLevel3 {
    type NextLevel = PageLevel2;
}

impl HierarchicalPageLevel for PageLevel2 {
    type NextLevel = PageLevel1;
}
