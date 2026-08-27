use core::{cmp::min, mem::size_of, ptr};

use alloc::{string::String, sync::Arc, vec::Vec};
use spin::{Mutex, RwLock};

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    elf::ElfRegion,
    fs::fs::{Directory, File, FileStat, S_IFCHR},
    io::LogType,
    log,
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

    /// When set, the process stays descheduled until the tick counter
    /// reaches this value.
    pub sleep_until_tick: Option<u64>,
    pub context: FullInterruptStackFrame,
    pub start_region: Arc<Mutex<ElfRegion>>,
    pub curr_working_dir: Arc<dyn Directory + Send + Sync>,
    pub stack: Stack,
    pub ring3_page_table: Option<PageTable>,
    pub fd_table: Vec<Option<FileDescriptor>>,

    /// The process environment, each entry a `KEY=VALUE` string. Handed to
    /// children through their entry stack frame when they are executed.
    pub env: Vec<String>,

    /// The lowest address the heap can occupy, one page past the end of the
    /// highest ELF segment. Also the break of a process that has never called
    /// `brk`, at which point the heap has no pages behind it at all.
    pub heap_start: usize,

    /// The current program break, recorded to the byte even though memory is
    /// handed out a page at a time.
    pub heap_break: usize,
}

/// `lseek` whence: the offset is absolute.
pub const SEEK_SET: usize = 0;

/// `lseek` whence: the offset is relative to the current position.
pub const SEEK_CUR: usize = 1;

/// `lseek` whence: the offset is relative to the end of the file.
pub const SEEK_END: usize = 2;

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

    /// Set by `O_APPEND`. Every write moves to the end of the file first,
    /// which has to happen per write rather than once at open, otherwise two
    /// descriptors appending to one file would overwrite each other.
    pub append: bool,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize, cwd: Arc<dyn Directory>) -> Option<Process> {
        let mut context = FullInterruptStackFrame::empty();
        context.rip = entry.entry;
        context.rsp = entry.initial_rsp;

        let Some(stack) = entry.stack else {
            log!(
                LogType::ERR,
                "process_from: missing stack for pid {}, entry: 0x{:X}",
                pid,
                entry.entry
            );

            return None;
        };

        if entry.ring3_page_table.is_none() {
            log!(
                LogType::ERR,
                "process_from: missing ring3 page table for pid {}, entry: 0x{:X}",
                pid,
                entry.entry
            );

            return None;
        }

        Some(Process {
            pid: pid,
            pre_schedule: true,
            blocking: false,
            awaiting_process: None,
            sleep_until_tick: None,
            context: context,
            start_region: entry.start_region,
            curr_working_dir: cwd,
            stack: stack,
            ring3_page_table: entry.ring3_page_table,
            fd_table: Self::standard_fd_table(),

            // deploy fills this in, either from the parent or from the
            // default environment
            env: Vec::new(),

            heap_start: entry.heap_start,
            heap_break: entry.heap_start,
        })
    }

    fn standard_fd_table() -> Vec<Option<FileDescriptor>> {
        let mut fd_table = Vec::new();
        fd_table.push(Some(FileDescriptor::Stdin));
        fd_table.push(Some(FileDescriptor::Stdout));
        fd_table.push(Some(FileDescriptor::Stderr));

        fd_table
    }

    /// Registers an open file and returns its descriptor.
    ///
    /// ## Arguments
    ///
    /// - `file` the file to register
    /// - `readable` whether reads are allowed
    /// - `writable` whether writes are allowed
    /// - `append` whether every write seeks to the end first
    pub fn open_file(
        &mut self,
        file: Arc<RwLock<dyn File>>,
        readable: bool,
        writable: bool,
        append: bool,
    ) -> usize {
        let descriptor = Some(FileDescriptor::File(OpenFile {
            file: file,
            offset: 0,
            readable: readable,
            writable: writable,
            append: append,
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

                // the size comes straight from userspace, so the buffer is
                // sized against what the file can actually supply rather than
                // against what was asked for
                let remaining = file.size().saturating_sub(open_file.offset);
                let to_read = min(size, remaining);
                if to_read == 0 {
                    return Some(Vec::new());
                }

                // only the requested window is pulled off the disk, reading
                // the whole file for every call made a buffered reader walk
                // the cluster chain from the start on every chunk
                let mut buffer = alloc::vec![0u8; to_read];
                let bytes_read = file.read_at(open_file.offset, &mut buffer)?;

                buffer.truncate(bytes_read);
                open_file.offset += bytes_read;

                Some(buffer)
            }
            _ => None,
        }
    }

    pub fn write_fd(&mut self, fd: usize, bytes: &[u8]) -> Option<usize> {
        let descriptor = self.fd_table.get_mut(fd)?.as_mut()?;
        match descriptor {
            FileDescriptor::File(open_file) => {
                if !open_file.writable {
                    return None;
                }

                let mut file = open_file.file.write();

                // O_APPEND is per write, not a one time seek at open
                if open_file.append {
                    open_file.offset = file.size();
                }

                let write_end = open_file.offset.checked_add(bytes.len())?;
                if write_end > file.size() {
                    file.truncate(write_end)?;
                }

                let bytes_written = file.write(open_file.offset, bytes)?;
                open_file.offset += bytes_written;

                Some(bytes_written)
            }
            _ => None,
        }
    }

    /// Metadata for an open file descriptor.
    ///
    /// ## Arguments
    ///
    /// - `fd` the descriptor to describe
    ///
    /// ## Returns
    /// The metadata, or `None` when the descriptor is not open.
    pub fn stat_fd(&self, fd: usize) -> Option<FileStat> {
        let descriptor = self.fd_table.get(fd)?.as_ref()?;
        match descriptor {
            FileDescriptor::File(open_file) => open_file.file.read().stat(),

            // the standard streams are the console, not files. stdio reads
            // this to pick its buffering, so they have to answer
            FileDescriptor::Stdin | FileDescriptor::Stdout | FileDescriptor::Stderr => {
                Some(FileStat {
                    mode: S_IFCHR,
                    links: 1,
                    block_size: 1,
                    ..FileStat::default()
                })
            }
        }
    }

    /// Moves the read/write offset of an open file descriptor.
    ///
    /// ## Arguments
    ///
    /// - `fd` the descriptor to seek
    /// - `offset` how far to move, relative to `whence`
    /// - `whence` [`SEEK_SET`], [`SEEK_CUR`] or [`SEEK_END`]
    ///
    /// ## Returns
    /// The new offset, or `None` when the descriptor is not a file, the
    /// whence is unknown, or the result would be negative.
    pub fn seek_fd(&mut self, fd: usize, offset: isize, whence: usize) -> Option<usize> {
        let descriptor = self.fd_table.get_mut(fd)?.as_mut()?;
        match descriptor {
            FileDescriptor::File(open_file) => {
                let base = match whence {
                    SEEK_SET => 0,
                    SEEK_CUR => open_file.offset,
                    SEEK_END => open_file.file.read().size(),
                    _ => return None,
                };

                // seeking past the end is allowed and leaves a hole that reads
                // as end of file, seeking before the start is not
                let new_offset = if offset >= 0 {
                    base.checked_add(offset as usize)?
                } else {
                    base.checked_sub(offset.unsigned_abs())?
                };

                open_file.offset = new_offset;
                Some(new_offset)
            }
            _ => None,
        }
    }

    pub fn truncate_fd(&mut self, fd: usize, size: usize) -> Option<()> {
        let descriptor = self.fd_table.get_mut(fd)?.as_mut()?;
        match descriptor {
            FileDescriptor::File(open_file) => {
                if !open_file.writable {
                    return None;
                }

                let mut file = open_file.file.write();
                file.truncate(size)?;

                if open_file.offset > size {
                    open_file.offset = size;
                }

                Some(())
            }
            _ => None,
        }
    }

    /// Checks whether a user pointer range is mapped with the required access.
    ///
    /// ## Arguments
    ///
    /// - `page_table` the process page table to validate against
    /// - `addr` the start virtual address of the range
    /// - `size` the size of the range in bytes
    /// - `writable` whether the range must be writable
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

    /// Checks whether a user pointer range can be accessed by a syscall.
    ///
    /// ## Arguments
    ///
    /// - `page_table` the process page table to validate against
    /// - `addr` the start virtual address of the range
    /// - `size` the size of the range in bytes
    /// - `writable` whether the range must be writable
    pub fn can_process_pointer(
        page_table: &PageTable,
        addr: usize,
        size: usize,
        writable: bool,
    ) -> bool {
        Self::validate_user_pointer(page_table, addr, size, writable)
    }

    /// Copies bytes from a validated user memory range into kernel memory.
    ///
    /// ## Arguments
    ///
    /// - `page_table` the process page table to validate against
    /// - `addr` the start virtual address to copy from
    /// - `size` the number of bytes to copy
    ///
    /// ## Returns
    /// A kernel-owned byte buffer if the user range is readable.
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

    /// Copies bytes from kernel memory into a validated user memory range.
    ///
    /// ## Arguments
    ///
    /// - `page_table` the process page table to validate against
    /// - `addr` the start virtual address to copy into
    /// - `bytes` the bytes to copy
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

    /// Copies a single value into a validated user memory range.
    ///
    /// ## Arguments
    ///
    /// - `page_table` the process page table to validate against
    /// - `addr` the start virtual address to copy into
    /// - `value` the value to copy
    pub fn copy_value_to_user<T>(page_table: &PageTable, addr: usize, value: &T) -> Option<()> {
        let size = size_of::<T>();
        if size == 0 {
            return Some(());
        }

        let src = value as *const T as *const u8;
        let bytes = unsafe { core::slice::from_raw_parts(src, size) };

        Self::copy_to_user(page_table, addr, bytes)
    }

    /// Copies a slice of values into a validated user memory range.
    ///
    /// ## Arguments
    ///
    /// - `page_table` the process page table to validate against
    /// - `addr` the start virtual address to copy into
    /// - `values` the values to copy
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

    /// The initial user stack pointer, pointing at the argument
    /// frame below the stack top.
    pub initial_rsp: usize,

    /// Where the heap begins, worked out from the segment addresses while the
    /// ELF was parsed.
    pub heap_start: usize,
}
