#![no_std]
#![no_main]

use core::{arch::global_asm, panic::PanicInfo};

use ulib::Args;

// runs on the kernel-provided stack, with the System V
// argument frame at the initial stack pointer
global_asm!(
    r#"
    .section .text
    .global _start

_start:
    mov rdi, [rsp]
    lea rsi, [rsp + 8]
    call rust_main

    mov rax, 1
    int 0x80

1:
    jmp 1b
"#
);

#[no_mangle]
extern "C" fn rust_main(argc: usize, argv: *const *const u8) -> ! {
    let args = Args::new(argc, argv);
    let Some(path) = args.get(1) else {
        ulib::stdout(b"Usage: cat <path>\n");
        ulib::exit();
    };

    let fd = ulib::open(path);
    if fd == 0 {
        ulib::stdout(b"cat: could not open ");
        ulib::stdout(path);
        ulib::stdout(b"\n");
        ulib::exit();
    }

    let mut buffer = [0u8; 512];
    loop {
        let bytes_read = ulib::read(fd, &mut buffer);
        if bytes_read == 0 {
            break;
        }

        ulib::stdout(&buffer[..bytes_read]);
    }

    ulib::close(fd);
    ulib::exit();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ulib::exit();
}
