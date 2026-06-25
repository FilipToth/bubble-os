use core::{mem::size_of, ptr};

use alloc::{sync::Arc, vec::Vec};
use spin::Mutex;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    elf::ElfRegion,
    fs::fs::Directory,
    mem::{
        paging::{entry::EntryFlags, PageTable},
        Stack, GLOBAL_MEMORY_CONTROLLER,
    },
};

#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,
    pub blocking: bool,
    pub awaiting_process: Option<usize>,
    pub context: FullInterruptStackFrame,
    pub start_region: Arc<Mutex<ElfRegion>>,
    pub curr_working_dir: Arc<dyn Directory + Send + Sync>,
    pub stack: Stack,
    pub ring3_page_table: Option<PageTable>,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize, cwd: Arc<dyn Directory>) -> Process {
        let mut context = FullInterruptStackFrame::empty();
        context.rip = entry.entry;

        Process {
            pid: pid,
            pre_schedule: true,
            blocking: false,
            awaiting_process: None,
            context: context,
            start_region: entry.start_region,
            curr_working_dir: cwd,
            stack: entry.stack.unwrap(),
            ring3_page_table: entry.ring3_page_table,
        }
    }

    pub fn validate_user_pointer(
        page_table: &PageTable,
        addr: usize,
        size: usize,
        writable: bool,
    ) -> bool {
        let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
        let Some(mc) = mc.as_mut() else {
            return false;
        };

        page_table
            .walk_range_entries(addr, size, &mut mc.temp_mapper, |_, entry| {
                let flags = entry.flags();
                let valid = flags.contains(EntryFlags::PRESENT)
                    && flags.contains(EntryFlags::RING3_ACCESSIBLE)
                    && (!writable || flags.contains(EntryFlags::WRITABLE));

                valid.then_some(())
            })
            .is_some()
    }

    pub fn can_process_pointer(
        page_table: &PageTable,
        addr: usize,
        size: usize,
        writable: bool,
    ) -> bool {
        Self::validate_user_pointer(page_table, addr, size, writable)
    }

    pub fn copy_from_user(page_table: &PageTable, addr: usize, size: usize) -> Option<Vec<u8>> {
        if !Self::validate_user_pointer(page_table, addr, size, false) {
            return None;
        }

        if size == 0 {
            return Some(Vec::new());
        }

        let slice = unsafe { core::slice::from_raw_parts(addr as *const u8, size) };
        let mut buffer = Vec::with_capacity(size);
        buffer.extend_from_slice(slice);

        Some(buffer)
    }

    pub fn copy_to_user(page_table: &PageTable, addr: usize, bytes: &[u8]) -> Option<()> {
        if !Self::validate_user_pointer(page_table, addr, bytes.len(), true) {
            return None;
        }

        if bytes.is_empty() {
            return Some(());
        }

        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
        }

        Some(())
    }

    pub fn copy_value_to_user<T>(page_table: &PageTable, addr: usize, value: &T) -> Option<()> {
        let size = size_of::<T>();
        if size == 0 {
            return Some(());
        }

        let src = value as *const T as *const u8;
        let bytes = unsafe { core::slice::from_raw_parts(src, size) };

        Self::copy_to_user(page_table, addr, bytes)
    }

    pub fn copy_slice_to_user<T>(page_table: &PageTable, addr: usize, values: &[T]) -> Option<()> {
        let size = size_of::<T>().checked_mul(values.len())?;

        if !Self::validate_user_pointer(page_table, addr, size, true) {
            return None;
        }

        if size == 0 {
            return Some(());
        }

        unsafe {
            ptr::copy_nonoverlapping(values.as_ptr() as *const u8, addr as *mut u8, size);
        }

        Some(())
    }
}

pub struct ProcessEntry {
    pub entry: usize,
    pub start_region: Arc<Mutex<ElfRegion>>,
    pub ring3_page_table: Option<PageTable>,
    pub stack: Option<Stack>,
}
