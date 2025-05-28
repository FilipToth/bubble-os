use core::alloc::Layout;

use alloc::{borrow::Cow, string::String};

#[derive(Clone)]
pub struct Region {
    pub ptr: *mut u8,
    pub size: usize,
}

impl Region {
    pub fn new(ptr: *mut u8, size: usize) -> Self {
        Self {
            ptr: ptr,
            size: size,
        }
    }

    pub fn to_string(&self) -> Cow<'_, str> {
        let slice = unsafe { core::slice::from_raw_parts(self.ptr, self.size) };
        String::from_utf8_lossy(slice)
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.size) }
    }

    pub fn as_slice_mut(&self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.size) }
    }

    pub fn construct_layout(&self) -> Layout {
        Layout::array::<u8>(self.size).unwrap()
    }
}
