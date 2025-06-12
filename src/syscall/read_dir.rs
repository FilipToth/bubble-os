use alloc::vec::Vec;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, fs::GLOBAL_FILESYSTEM, mem::paging::entry,
    print,
};

/// Simplified version of the FAT directory entry
#[repr(C)]
struct SyscallDirEntry {
    name: [u8; 64],
    attr: u8,
    size: u32,
}

pub fn read_dir(stack: &FullInterruptStackFrame) -> Option<usize> {
    let path_addr = stack.rdi;
    let path_len = stack.rsi;
    let buffer_addr = stack.rdx;
    let max_items = stack.rcx;

    let mut fs = GLOBAL_FILESYSTEM.lock();
    let fs = fs.as_mut().unwrap();

    let entries = if path_len != 0 {
        let slice = unsafe { core::slice::from_raw_parts(path_addr as *const u8, path_len) };
        let path = core::str::from_utf8(slice).unwrap_or("Invalid string for execute syscall\n");

        // complicated, get entries by path
        unimplemented!()
    } else {
        fs.list_root_dir()
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
    let entry_size = core::mem::size_of::<SyscallDirEntry>();

    let num_entries = entries.len();
    for entry in entries.iter() {
        unsafe {
            core::ptr::copy(entry as *const SyscallDirEntry, buffer_ptr, 1);
            buffer_ptr = buffer_ptr.add(1);
        }
    }

    Some(num_entries)
}
