use multiboot2::{ElfSection, ElfSectionFlags};

use crate::mem::PageFrame;

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct EntryFlags: u64 {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const RING3_ACCESSIBLE = 1 << 2;
        const WRITE_DIRECTLY_TO_MEM = 1 << 3;
        const CACHE_DISABLE = 1 << 4;
        const ACCESSED = 1 << 5;
        const DIRTY = 1 << 6;
        const HUGE_PAGE = 1 << 7;
        const GLOBAL = 1 << 8;
        const NO_EXECUTE = 1 << 63;
    }
}

impl EntryFlags {
    pub fn from_elf_section_flags(section: &ElfSection) -> EntryFlags {
        let mut flags = EntryFlags::empty();

        if section.flags().contains(ElfSectionFlags::ALLOCATED) {
            // loaded in memory
            flags |= EntryFlags::PRESENT;
        }

        if section.flags().contains(ElfSectionFlags::WRITABLE) {
            flags |= EntryFlags::WRITABLE;
        }

        if !section.flags().contains(ElfSectionFlags::EXECUTABLE) {
            flags |= EntryFlags::NO_EXECUTE;
        }

        flags
    }
}

#[derive(Clone)]
pub struct PageTableEntry {
    pub entry: u64,
}

impl PageTableEntry {
    pub fn is_unused(&self) -> bool {
        self.entry == 0
    }

    pub fn set_to_unused(&mut self) {
        self.entry = 0;
    }

    pub fn flags(&self) -> EntryFlags {
        EntryFlags::from_bits_truncate(self.entry)
    }

    pub fn get_frame(&self) -> Option<PageFrame> {
        if self.flags().contains(EntryFlags::PRESENT) {
            Some(PageFrame::from_address(
                (self.entry as usize) & 0x000FFFFF_FFFFF000,
            ))
        } else {
            None
        }
    }

    pub fn set(&mut self, frame: PageFrame, flags: EntryFlags) {
        // make sure we have a valid address, if not, this is an os bug
        let addr = frame.start_address();
        assert!(addr & !0x000FFFFF_FFFFF000 == 0);
        self.entry = (addr as u64) | flags.bits();
    }
}
