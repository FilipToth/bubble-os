// syscall 7 - read current directory entries into a user buffer

use core::mem::size_of;

use alloc::vec::Vec;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, scheduling, scheduling::process::Process,
};

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
    let Some(page_table) = scheduling::get_current_process_page_table() else {
        return Some(0);
    };

    let Some(buffer_size) = max_items.checked_mul(size_of::<SyscallDirEntry>()) else {
        return Some(0);
    };

    if !Process::can_process_pointer(&page_table, buffer_addr, buffer_size, true) {
        return Some(0);
    }

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

    let remaining_items = max_items.saturating_sub(directory_entries.len());
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
        .take(remaining_items)
        .collect();

    directory_entries.extend(file_entries);

    let num_entries = directory_entries.len();
    if Process::copy_slice_to_user(&page_table, buffer_addr, &directory_entries).is_none() {
        return Some(0);
    }

    Some(num_entries)
}
