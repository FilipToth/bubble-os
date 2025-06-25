use alloc::boxed::Box;
use fat_fs::FATFileSystem;
use spin::Mutex;

use crate::ahci::port::AHCIPort;

pub mod fat;
mod fat_fs;

pub static GLOBAL_FILESYSTEM: Mutex<Option<FATFileSystem>> = Mutex::new(None);

/// Initializes a new singular FAT32 filesystem.
/// For now, bubble-os is only designed to handle
/// a singular filesystem for the entire system.
pub fn init(port: Box<AHCIPort>) {
    let fs = FATFileSystem::new(port).unwrap();

    let mut guard = GLOBAL_FILESYSTEM.lock();
    *guard = Some(fs);
}
