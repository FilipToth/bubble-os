use core::any::Any;

use alloc::boxed::Box;
use fat_fs::FATFileSystem;
use fs::{Directory, File, FileSystem};
use spin::Mutex;

use crate::ahci::port::AHCIPort;

pub mod fat;
pub mod fat_fs;
pub mod fs;

type BoxedFS = Box<
    dyn FileSystem<
            FileType = Box<dyn File + Send + Sync>,
            DirectoryType = Box<dyn Directory + Send + Sync>,
        > + Send
        + Sync,
>;

pub static GLOBAL_FILESYSTEM: Mutex<Option<Box<dyn Any + Send + Sync>>> = Mutex::new(None);

#[macro_export]
macro_rules! with_fs {
    ($t:ty, $var:ident, $body:block) => {{
        let mut guard = $crate::GLOBAL_FILESYSTEM.lock();
        if let Some(fs) = guard.as_mut().and_then(|b| b.downcast_mut::<$t>()) {
            let $var = fs;
            $body
        } else {
            panic!();
        }
    }};
}

/// Initializes a new singular FAT32 filesystem.
/// For now, bubble-os is only designed to handle
/// a singular filesystem for the entire system.
pub fn init(port: AHCIPort) {
    let fs = FATFileSystem::new(Box::new(port)).unwrap();
    let mut guard = GLOBAL_FILESYSTEM.lock();
    *guard = Some(Box::new(fs));
}
