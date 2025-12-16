use core::ops::Add;

use crate::mem::{
    paging::{entry::EntryFlags, slot_allocator::PageTableSlotAllocator, temp_mapper::TempMapper},
    PageFrame, PageFrameAllocator, VirtualAddress, PAGE_SIZE,
};
use multiboot2::BootInformation;
pub use page_table::PageTable;
use x86_64::{
    registers::control::{Cr3, Cr3Flags},
    structures::paging::PhysFrame,
    PhysAddr,
};

pub mod entry;
mod page_table;
pub mod slot_allocator;
pub mod temp_mapper;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Page {
    page_number: usize,
}

impl Page {
    /// Instantiates a new unmapped page to for corresponding
    /// virtual address.
    ///
    /// ## Arguments
    ///
    /// - `addr` the virtual address for the page to be
    /// mapped on to
    pub fn for_address(addr: VirtualAddress) -> Page {
        // assert!(addr < 0x0000_8000_0000_0000 || addr >= 0xFFFF_8000_0000_0000);
        Page {
            page_number: addr / PAGE_SIZE,
        }
    }

    pub fn start_address(&self) -> VirtualAddress {
        self.page_number * PAGE_SIZE
    }

    pub fn p4_index(&self) -> usize {
        (self.page_number >> 27) & 0o777
    }

    pub fn p3_index(&self) -> usize {
        (self.page_number >> 18) & 0o777
    }

    pub fn p2_index(&self) -> usize {
        (self.page_number >> 9) & 0o777
    }

    pub fn p1_index(&self) -> usize {
        (self.page_number >> 0) & 0o777
    }

    pub fn range(start: Page, end: Page) -> PageIter {
        PageIter {
            start: start,
            end: end,
        }
    }
}

impl Add<usize> for Page {
    type Output = Page;

    fn add(self, rhs: usize) -> Self::Output {
        Page {
            page_number: self.page_number + rhs,
        }
    }
}

#[derive(Clone)]
pub struct PageIter {
    start: Page,
    end: Page,
}

impl Iterator for PageIter {
    type Item = Page;

    fn next(&mut self) -> Option<Page> {
        if self.start > self.end {
            return None;
        }

        let frame = self.start.clone();
        self.start.page_number += 1;
        Some(frame)
    }
}

pub fn switch_table(new_table: &PageTable) {
    // TODO: Handle when addr is a virtual address, Cr3 expects physical addresses
    let addr = new_table.addr;
    let phys_addr = PhysAddr::new(addr as u64);
    let phys_frame = PhysFrame::from_start_address(phys_addr)
        .expect("Cannot create cr3 new frame swap address.");

    unsafe { Cr3::write(phys_frame, Cr3Flags::empty()) };
}

pub fn map_kernel<A>(
    allocator: &mut A,
    slot_allocator: &mut PageTableSlotAllocator,
    pml4: &mut PageTable,
    boot_info: &BootInformation,
    temp_mapper: &mut TempMapper,
) where
    A: PageFrameAllocator,
{
    let multiboot_start = PageFrame::from_address(boot_info.start_address());
    let multiboot_end = PageFrame::from_address(boot_info.end_address() - 1);

    pml4.map_range_identity(
        multiboot_start,
        multiboot_end,
        EntryFlags::PRESENT,
        allocator,
        slot_allocator,
        temp_mapper,
    );

    let elf_sections = boot_info.elf_sections().unwrap();
    for section in elf_sections {
        if !section.is_allocated() {
            // not loaded in memory :(
            continue;
        }

        // check page alignment
        let aligned = (section.start_address() as usize) % PAGE_SIZE == 0;
        assert!(aligned, "ELF Sections need to be aligned to the page size");

        // need to offset the end frame by one to prevent having the end frame
        // and the starting frame of the next elf section from being the same
        // and the page already being used, thus failing an assert when mapping...

        let flags = EntryFlags::from_elf_section_flags(&section);
        let start_frame = PageFrame::from_address(section.start_address() as usize);
        let end_frame = PageFrame::from_address((section.end_address() - 1) as usize);

        pml4.map_range_identity(
            start_frame,
            end_frame,
            flags,
            allocator,
            slot_allocator,
            temp_mapper,
        );
    }
}
