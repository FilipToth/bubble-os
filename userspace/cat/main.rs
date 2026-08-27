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
    lea rdx, [rsi + rdi*8 + 8]
    call rust_main

    xor edi, edi
    mov rax, 1
    int 0x80

1:
    jmp 1b
"#
);

#[no_mangle]
extern "C" fn rust_main(argc: usize, argv: *const *const u8, envp: *const *const u8) -> ! {
    ulib::set_environ(envp);

    let args = Args::new(argc, argv);
    let Some(path) = args.get(1) else {
        ulib::stdout(b"Usage: cat <path>\n");
        ulib::exit(1);
    };

    let fd = match ulib::open(path, ulib::O_RDONLY) {
        Ok(fd) => fd,
        Err(error) => {
            ulib::stdout(b"cat: could not open ");
            ulib::stdout(path);
            ulib::stdout(b": ");
            ulib::stdout(error.as_str().as_bytes());
            ulib::stdout(b"\n");
            ulib::exit(1);
        }
    };

    let mut buffer = [0u8; 512];
    loop {
        let bytes_read = match ulib::read(fd, &mut buffer) {
            // a zero length read is end of file, an error is its own answer
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                ulib::stdout(b"cat: read failed: ");
                ulib::stdout(error.as_str().as_bytes());
                ulib::stdout(b"\n");
                let _ = ulib::close(fd);
                ulib::exit(1);
            }
        };

        ulib::stdout(&buffer[..bytes_read]);
    }

    let _ = ulib::close(fd);
    ulib::exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ulib::exit(101);
}
