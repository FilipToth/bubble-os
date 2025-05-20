use crate::print;

pub fn write() {
    let file_descriptor: usize;
    let buffer_addr: usize;
    let buffer_size: usize;

    unsafe {
        core::arch::asm!(
            "",
            lateout("rdi") file_descriptor,
            lateout("rsi") buffer_addr,
            lateout("r11") buffer_size
        );
    }

    let slice = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, buffer_size) };
    let string = core::str::from_utf8(slice).unwrap_or("Invalid string for write syscall");

    match file_descriptor {
        1 => {
            // stdout write
            print!("{}", string);
        }
        _ => {}
    }
}
