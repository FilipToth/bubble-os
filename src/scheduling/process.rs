use core::{cmp::min, mem::size_of, ptr};

use alloc::{sync::Arc, vec::Vec};
use spin::{Mutex, RwLock};

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    elf::ElfRegion,
    fs::fs::{Directory, File},
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
    pub fd_table: Vec<Option<FileDescriptor>>,
}

#[derive(Clone)]
pub enum FileDescriptor {
    Stdin,
    Stdout,
    Stderr,
    File(OpenFile),
}

#[derive(Clone)]
pub struct OpenFile {
    pub file: Arc<RwLock<dyn File>>,
    pub offset: usize,
    pub readable: bool,
    pub writable: bool,
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
            fd_table: Self::standard_fd_table(),
        }
    }

    fn standard_fd_table() -> Vec<Option<FileDescriptor>> {
        let mut fd_table = Vec::new();
        fd_table.push(Some(FileDescriptor::Stdin));
        fd_table.push(Some(FileDescriptor::Stdout));
        fd_table.push(Some(FileDescriptor::Stderr));

        fd_table
    }

    pub fn open_file(
        &mut self,
        file: Arc<RwLock<dyn File>>,
        readable: bool,
        writable: bool,
    ) -> usize {
        let descriptor = Some(FileDescriptor::File(OpenFile {
            file: file,
            offset: 0,
            readable: readable,
            writable: writable,
        }));

        for fd in 3..self.fd_table.len() {
            if self.fd_table[fd].is_none() {
                self.fd_table[fd] = descriptor.clone();
                return fd;
            }
        }

        self.fd_table.push(descriptor);
        self.fd_table.len() - 1
    }

    pub fn close_fd(&mut self, fd: usize) -> bool {
        if fd < 3 || fd >= self.fd_table.len() {
            return false;
        }

        self.fd_table[fd] = None;
        true
    }

    pub fn get_fd(&self, fd: usize) -> Option<&FileDescriptor> {
        self.fd_table.get(fd)?.as_ref()
    }

    pub fn read_fd(&mut self, fd: usize, size: usize) -> Option<Vec<u8>> {
        let descriptor = self.fd_table.get_mut(fd)?.as_mut()?;
        match descriptor {
            FileDescriptor::File(open_file) => {
                if !open_file.readable {
                    return None;
                }

                let file = open_file.file.read();
                let file_region = file.read()?;
                let file_bytes = file_region.as_slice();

                if open_file.offset >= file_bytes.len() {
                    return Some(Vec::new());
                }

                let requested_end = open_file
                    .offset
                    .checked_add(size)
                    .unwrap_or(file_bytes.len());
                let end = min(requested_end, file_bytes.len());
                let bytes = &file_bytes[open_file.offset..end];
                open_file.offset = end;

                let mut buffer = Vec::with_capacity(bytes.len());
                buffer.extend_from_slice(bytes);

                Some(buffer)
            }
            _ => None,
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
