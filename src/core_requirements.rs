/// libc `memcpy` implementation
/// 
/// # Parameters
/// 
/// * `dest` - Memory pointer to copy from
/// * `src`  - Memory pointer to copy to
/// * `n`    - Number of bytes to copy
#[no_mangle]
pub unsafe extern fn memcpy(dest: *mut u8, src: *const u8, n: usize) {
    for i in 0..n {
        *dest.offset(i as isize) = *src.offset(i as isize);
    }
}

/// libc `memmove` implementation
/// 
/// # Parameters
/// * - `dest` - Memory pointer to move to
/// * - `src`  - Memory pointer to move from
/// * - `n`    - Number of bytes to move
#[no_mangle]
pub unsafe extern fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if src < dest as *const u8 {
        for i in (0..n).rev() {
            *dest.offset(i as isize) = *src.offset(i as isize);
        }
    } else {
        for i in 0..n {
            *dest.offset(i as isize) = *src.offset(i as isize);
        }
    }

    dest
}

/// libc `memset` implementation
/// 
/// # Parameters
/// 
/// * - `s` - Memory pointer to set
/// * - `c` - Value to set memory to
/// * - `n` - Number of bytes to set
#[no_mangle]
pub unsafe extern fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        *s.offset(i as isize) = c as u8;
    }

    s
}

/// libc `memcmp` implementation
/// 
/// # Parameters
/// 
/// * - `s1` - Memory pointer to compare
/// * - `s2` - Memory pointer to compare
/// * - `n`  - Number of bytes to compare
#[no_mangle]
pub unsafe extern fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        if *s1.offset(i as isize) != *s2.offset(i as isize) {
            return *s1.offset(i as isize) as i32 - *s2.offset(i as isize) as i32;
        }
    }
    0
}