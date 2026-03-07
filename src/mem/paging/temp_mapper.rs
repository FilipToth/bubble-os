use x86_64::{instructions::tlb, structures::paging::PhysFrame, VirtAddr};

use crate::mem::{
    paging::entry::{EntryFlags, PageTableEntry},
    PageFrame, PAGE_SIZE,
};

pub struct TempMapper {
    temp_addr: usize,
    temp_entry_addr: usize,
}

impl TempMapper {
    pub fn new(temp_addr: usize, temp_entry_addr: usize) -> Self {
        Self {
            temp_addr: temp_addr,
            temp_entry_addr: temp_entry_addr,
        }
    }

    pub fn deref<T>(&mut self, addr: usize) -> &'static mut T {
        let offset = addr & (PAGE_SIZE - 1);
        let size = core::mem::size_of::<T>();

        if (offset + size) > PAGE_SIZE {
            // size must be less than size of one page
            unreachable!()
        }

        // set temp mapping to phys frame
        let phys = PageFrame::from_address(addr);
        self.set(phys);

        let item_va = self.temp_addr + offset;
        let item_ptr = item_va as *mut T;
        unsafe { &mut *item_ptr }
    }

    pub fn set(&mut self, phys: PageFrame) -> usize {
        let entry_ptr = self.temp_entry_addr as *mut PageTableEntry;
        let entry = unsafe { &mut *entry_ptr };

        entry.set(phys, EntryFlags::PRESENT | EntryFlags::WRITABLE);

        let temp_va = VirtAddr::new(self.temp_addr as u64);
        tlb::flush(temp_va);

        self.temp_addr
    }

    pub fn get_current_phys(&self) -> Option<PageFrame> {
        // TODO: maybe cache this?

        let entry_ptr = self.temp_entry_addr as *mut PageTableEntry;
        let entry = unsafe { &mut *entry_ptr };

        entry.get_frame()
    }
}
