use alloc::vec::Vec;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    fs::{
        fat_fs::{FATDirectory, FATFileSystem},
        fs::{Directory, DirectoryKind, File, FileSystem},
    },
    scheduling, with_fs,
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

    let cwd = scheduling::get_current_cwd();
    let entries = match cwd {
        DirectoryKind::FATDirectory(dir) => {
            with_fs!(FATFileSystem, fs, {
                fs.list_directory(&dir)
            })
        }
    };

    let mut syscall_entries: Vec<SyscallDirEntry> = entries
        .directories
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
        .files
        .iter()
        .map(|e| {
            let mut name_buffer: [u8; 64] = [0; 64];
            let name = e.name();
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

    syscall_entries.extend(file_entries);

    // write entries into supplied entries buffer
    let mut buffer_ptr = buffer_addr as *mut SyscallDirEntry;
    let num_entries = syscall_entries.len();

    for entry in syscall_entries.iter() {
        unsafe {
            core::ptr::copy(entry as *const SyscallDirEntry, buffer_ptr, 1);
            buffer_ptr = buffer_ptr.add(1);
        }
    }

    Some(num_entries)
}
