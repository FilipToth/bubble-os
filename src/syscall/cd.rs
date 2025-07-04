use alloc::format;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, fs::{fat_fs::FATFileSystem, fs::{Directory, DirectoryKind, FileSystem}}, print, scheduling, with_fs,
};

pub fn cd(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let dirname = match core::str::from_utf8(slice) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for change directory syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            print!("{}\n{:?}\n", msg, e);
            return Some(0);
        }
    };

    let dirname = dirname.trim();

    let cwd = scheduling::get_current_cwd();
    let new_dir = match cwd {
        DirectoryKind::FATDirectory(dir) => {
            with_fs!(FATFileSystem, fs, {
                let entries = fs.list_directory(&dir);
                entries.directories.iter().find(|d| d.name() == dirname)?.clone()
            })
        }
    };

    print!("New FAT Directory: {:#?}\n", new_dir.entry);

    let new_dir_kind = DirectoryKind::FATDirectory(new_dir);
    scheduling::change_cwd(new_dir_kind);

    None
}
