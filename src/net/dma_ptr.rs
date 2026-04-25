#[derive(Clone, Copy)]
pub struct DMAPtr<T> {
    ptr: *mut T,
}

unsafe impl<T> Send for DMAPtr<T> {}
unsafe impl<T> Sync for DMAPtr<T> {}

impl<T> DMAPtr<T> {
    pub const fn null() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
        }
    }

    pub const fn from_ptr(ptr: *mut T) -> Self {
        Self { ptr: ptr }
    }

    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }

    pub unsafe fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    pub unsafe fn add(&self, count: usize) -> *mut T {
        self.ptr.add(count)
    }
}
