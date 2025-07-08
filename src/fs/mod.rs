use alloc::{boxed::Box, sync::Arc};
use fat_fs::FATFileSystem;
use fs::Directory;
use spin::Mutex;

use crate::{ahci::port::AHCIPort, print};

pub mod fat;
pub mod fat_fs;
pub mod fs;

// Need make sure the FS Arc lives statically so that the
// weak references inside of individual directories and
// files don't get invalidated
static GLOBAL_FS: Mutex<Option<Arc<Mutex<FATFileSystem>>>> = Mutex::new(None);

pub static GLOBAL_ROOT_DIR: Mutex<Option<Arc<dyn Directory + Send + Sync>>> =
    Mutex::new(None);

#[macro_export]
macro_rules! with_root_dir {
    ($var:ident, $body:block) => {{
        let __guard = $crate::fs::GLOBAL_ROOT_DIR.lock();
        if let Some(dir) = __guard.as_ref() {
            let $var = dir.clone();
            $body
        } else {
            panic!("Global root directory not initialized");
        }
    }};
}

pub fn init(port: AHCIPort) {
    let fs = FATFileSystem::new(Box::new(port)).unwrap();
    let fs_arc = Arc::new(Mutex::new(fs));
    let root = FATFileSystem::root_dir(fs_arc.clone());

    *GLOBAL_FS.lock() = Some(fs_arc);

    let mut guard = GLOBAL_ROOT_DIR.lock();
    *guard = Some(Arc::new(root));
}
