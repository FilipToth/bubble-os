pub unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "outb %al, %dx",
        in("dx") port,
        in("al") value,
        options(att_syntax)
    );
}

pub fn inb(port: u16) -> u8 {
    let ret: u8;
    unsafe {
        core::arch::asm!(
            "inb %dx, %al",
            out("al") ret,
            in("dx") port,
            options(att_syntax)
        );
    }

    ret
}

// http://www.randomhacks.net/2015/11/09/bare-metal-rust-cpu-port-io/
// https://stackoverflow.com/questions/64815258/what-is-the-correct-rust-inline-assembly-format
