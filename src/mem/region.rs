use core::alloc::Layout;

use alloc::{borrow::Cow, string::String};

#[derive(Clone)]
pub struct Region {
    pub addr: usize,
    pub size: usize,
}

impl Region {
    pub fn new(addr: usize, size: usize) -> Self {
        Self {
            addr: addr,
            size: size,
        }
    }

    pub fn from<T>(ptr: *mut T, size: usize) -> Self {
        let addr = ptr as usize;
        Region::new(addr, size)
    }

    pub fn get_ptr<T>(&self) -> *mut T {
        self.addr as *mut T
    }

    pub fn to_string(&self) -> Cow<'_, str> {
        let ptr = self.get_ptr::<u8>();
        let slice = unsafe { core::slice::from_raw_parts(ptr, self.size) };
        String::from_utf8_lossy(slice)
    }

    pub fn as_slice(&self) -> &[u8] {
        let ptr = self.get_ptr::<u8>();
        unsafe { core::slice::from_raw_parts(ptr, self.size) }
    }

    pub fn as_slice_mut(&self) -> &mut [u8] {
        let ptr = self.get_ptr::<u8>();
        unsafe { core::slice::from_raw_parts_mut(ptr, self.size) }
    }

    pub fn construct_layout(&self) -> Layout {
        Layout::array::<u8>(self.size).unwrap()
    }
}
