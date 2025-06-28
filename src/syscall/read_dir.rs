use alloc::vec::Vec;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, fs::GLOBAL_FILESYSTEM, print, scheduling,
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

    let mut fs = GLOBAL_FILESYSTEM.lock();
    let fs = fs.as_mut().unwrap();

    let cwd = scheduling::get_current_cwd();
    let entries = match fs.resolve_path(cwd) {
        Some(entry) => {
            let cluster = entry.get_cluster();
            fs.read_directory(cluster)
        }
        None => {
            // root dir
            fs.list_root_dir()
        }
    };

    let entries: Vec<SyscallDirEntry> = entries
        .iter()
        .map(|e| {
            let mut name_buffer: [u8; 64] = [0; 64];
            let name = e.get_filename();
            let name_len = name.len().min(64);
            let name = name.as_bytes();

            name_buffer[..name_len].copy_from_slice(&name[..name_len]);

            SyscallDirEntry {
                name: name_buffer,
                attr: e.attributes,
                size: e.size,
            }
        })
        .take(max_items)
        .collect();

    // write entries into supplied entries buffer
    let mut buffer_ptr = buffer_addr as *mut SyscallDirEntry;
    let num_entries = entries.len();

    for entry in entries.iter() {
        unsafe {
            core::ptr::copy(entry as *const SyscallDirEntry, buffer_ptr, 1);
            buffer_ptr = buffer_ptr.add(1);
        }
    }

    Some(num_entries)
}
