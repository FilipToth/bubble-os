use alloc::vec::Vec;

use crate::{arch::x86_64::registers::FullInterruptStackFrame, scheduling};

/// Simplified version of the FAT directory entry
#[repr(C)]
struct SyscallDirEntry {
    name: [u8; 64],
    attr: u8,
    size: u32,
}

pub fn read_dir(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let max_items = stack.rsi;

    let cwd = scheduling::get_current_cwd();
    let entries = cwd.list_dir();

    let mut directory_entries: Vec<SyscallDirEntry> = entries
        .0
        .iter()
        .map(|e| {
            let mut name_buffer: [u8; 64] = [0; 64];
            let name = e.name();
            let name_len = name.len().min(64);
            let name = name.as_bytes();

            name_buffer[..name_len].copy_from_slice(&name[..name_len]);

            SyscallDirEntry {
                name: name_buffer,
                attr: 0x10,
                size: 0,
            }
        })
        .take(max_items)
        .collect();

    let file_entries: Vec<SyscallDirEntry> = entries
        .1
        .iter()
        .map(|e| {
            let mut name_buffer: [u8; 64] = [0; 64];
            let name = e.read().name();
            let name_len = name.len().min(64);
            let name = name.as_bytes();

            name_buffer[..name_len].copy_from_slice(&name[..name_len]);

            SyscallDirEntry {
                name: name_buffer,
                attr: 0,
                size: 0,
            }
        })
        .take(max_items)
        .collect();

    directory_entries.extend(file_entries);

    // write entries into supplied entries buffer
    let mut buffer_ptr = buffer_addr as *mut SyscallDirEntry;
    let num_entries = directory_entries.len();

    for entry in directory_entries.iter() {
        unsafe {
            core::ptr::copy(entry as *const SyscallDirEntry, buffer_ptr, 1);
            buffer_ptr = buffer_ptr.add(1);
        }
    }

    Some(num_entries)
}
