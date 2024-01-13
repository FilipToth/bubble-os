use core::ops::{Deref, DerefMut};

use multiboot2::BootInformation;
use x86_64::instructions::tlb;
use x86_64::registers::control;

use crate::mem::{PAGE_SIZE, PageFrame};
use crate::mem::paging::entry::EntryFlags;
use crate::mem::VirtualAddress;
use crate::print;

pub mod entry;
pub mod temp_page;
pub mod page_table;
pub mod page_mapper;

use self::page_mapper::Mapper;
use self::temp_page::TempPage;

use super::PageFrameAllocator;

const TABLE_ENTRY_COUNT: usize = 512;

#[derive(Debug, Clone, Copy)]
pub struct Page {
    page_number: usize
}

impl Page {
    /// Instantiates a new unmapped page to for corresponding
    /// virtual address.
    /// 
    /// # Arguments
    /// 
    /// - `addr` the virtual address for the page to be
    /// mapped on to
    pub fn for_address(addr: VirtualAddress) -> Page {
        assert!(addr < 0x0000_8000_0000_0000 || addr >= 0xFFFF_8000_0000_0000);
        Page { page_number: addr / PAGE_SIZE }
    }

    pub fn start_address(&self) -> VirtualAddress {
        self.page_number * PAGE_SIZE
    }

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
}

/// A page table that is currently loaded in the CPU
pub struct ActivePageTable {
    mapper: Mapper
}

impl ActivePageTable {
    unsafe fn new() -> ActivePageTable {
        ActivePageTable { mapper: Mapper::new() }
    }
    
    /// Calls the provided closure on the
    /// Inactive page table. Loads the table
    /// into the recursive map.
    /// 
    /// # Arguments
    /// 
    /// - `temp_page` requires a temporary
    /// page to store a backup to the old
    /// mappings in order to restore them
    /// - `table` the inactive page table
    /// to load
    /// - `f` the closure to call
    pub fn with<F>(&mut self, temp_page: &mut TempPage, table: &mut InactivePageTable, f: F)
        where F: FnOnce(&mut Mapper)
    {
        {
            // maybe this works, maybe it doesn't...
            let active_table_addr = control::Cr3::read().0
                                                .start_address()
                                                .as_u64() as usize;
            
            // need this to restore active table
            // after calling the supplied closure
            let p4_backup = PageFrame::from_address(active_table_addr);
            let p4_table = temp_page.map_table_frame(p4_backup.clone(), self);

            // overwrite the recursive mapping
            self.get_p4_mut()[511].set(table.p4_frame.clone(), EntryFlags::PRESENT | EntryFlags::WRITABLE);
            tlb::flush_all();

            f(self);

            // restore mappings to the active p4 table
            p4_table[511].set(p4_backup, EntryFlags::PRESENT | EntryFlags::WRITABLE);
            tlb::flush_all();

            // inner scope drops the temp page
        }
        
        temp_page.unmap(self);
    }
}

impl Deref for ActivePageTable {
    type Target = Mapper;
    
    fn deref(&self) -> &Self::Target {
        &self.mapper
    }   
}

impl DerefMut for ActivePageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mapper
    }
}

/// A page table which isn't loaded in the CPU.
pub struct InactivePageTable {
    p4_frame: PageFrame
}

impl InactivePageTable {
    /// Instantiates an inactive page table
    /// for a frame to be used for the p4.
    /// 
    /// # Arguments
    /// 
    /// - `frame` the frame to be used for the p4
    pub fn new(frame: PageFrame, active_table: &mut ActivePageTable, temp_page: &mut TempPage) -> InactivePageTable {
        // we need to null the frame, but the frame
        // isn't yet mapped to a virtual address,
        // therefore we need to create a temporary
        // mapping.
        
        {
            let table = temp_page.map_table_frame(frame.clone(), active_table);

            table.null_all_entries();
            table[511].set(frame.clone(), EntryFlags::PRESENT | EntryFlags::WRITABLE);

            // inner scope drops the table
        }

        temp_page.unmap(active_table);
        InactivePageTable { p4_frame: frame }
    }
}

pub fn remap_kernel<A>(allocator: &mut A, boot_info: &BootInformation)
    where A: PageFrameAllocator
{
    let mut temp_page = TempPage::new(Page { page_number: 0xA0000ABC }, allocator);

    let active_table = unsafe {
        ActivePageTable::new()
    };

    let inactive_table = {
        let frame = allocator.falloc().expect("Cannot allocate more frames");
        InactivePageTable::new(frame, &mut active_table, &mut temp_page)
    };

    active_table.with(&mut temp_page, &mut inactive_table, |mapper| {
        let elf_sections = boot_info.elf_sections().unwrap();
        for section in elf_sections {
            if !section.is_allocated() {
                // not loaded in memory :(
                continue;
            }

            // check page alignment
            let aligned = (section.start_address() as usize) % PAGE_SIZE == 0;
            assert!(aligned, "ELF Sections need to be aligned to the page size");

            print!("Creating elf mapping: 0x{:#x}, size: 0x{:#x}", section.start_address(), section.size());

            let flags = EntryFlags::WRITABLE; // ::PRESENT will be set automaitcally
            let start_frame = PageFrame::from_address(section.start_address() as usize);
            let end_frame = PageFrame::from_address(section.end_address() as usize);

            for frame in PageFrame::range(start_frame, end_frame) {
                mapper.map_identity(frame, flags, allocator);
            }
        }
    });
}
