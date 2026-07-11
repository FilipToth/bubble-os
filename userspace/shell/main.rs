#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(
    r#"
    .section .bss
    .align 16

stack_bottom:
    .skip 4096

stack_top:
    .section .text
    .global _start

_start:
    lea rsp, [rip + stack_top]
    call rust_main

    mov rax, 1
    int 0x80

1:
    jmp 1b
"#
);

#[no_mangle]
extern "C" fn rust_main() -> ! {
    let mut input_buffer = [0u8; 128];

    ulib::stdout(br#"
 _______             __        __        __                   ______    ______
/       \           /  |      /  |      /  |                 /      \  /      \
$$$$$$$  | __    __ $$ |____  $$ |____  $$ |  ______        /$$$$$$  |/$$$$$$  |
$$ |__$$ |/  |  /  |$$      \ $$      \ $$ | /      \       $$ |  $$ |$$ \__$$/
$$    $$< $$ |  $$ |$$$$$$$  |$$$$$$$  |$$ |/$$$$$$  |      $$ |  $$ |$$      \
$$$$$$$  |$$ |  $$ |$$ |  $$ |$$ |  $$ |$$ |$$    $$ |      $$ |  $$ | $$$$$$  |
$$ |__$$ |$$ \__$$ |$$ |__$$ |$$ |__$$ |$$ |$$$$$$$$/       $$ \__$$ |/  \__$$ |
$$    $$/ $$    $$/ $$    $$/ $$    $$/ $$ |$$       |      $$    $$/ $$    $$/
$$$$$$$/   $$$$$$/  $$$$$$$/  $$$$$$$/  $$/  $$$$$$$/        $$$$$$/   $$$$$$/

"#);

    ulib::stdout(b"\nWelcome to the Bubble OS Kernel Shell :D\n\n");

    loop {
        ulib::stdout(b"$ ");

        let input_len = read_command(&mut input_buffer);
        ulib::stdout(b"\n");

        if input_len == 0 {
            continue;
        }

        let command = &input_buffer[..input_len];
        if command.starts_with(b"cd ") {
            ulib::cd(&command[3..]);
            continue;
        }

        let pid = ulib::execute(command);
        if pid == 0 {
            ulib::stdout(b"Program or command not found...\n");
            continue;
        }

        ulib::wait_for_process(pid);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ulib::exit();
}

fn read_command(buffer: &mut [u8]) -> usize {
    let mut len = 0;

    loop {
        let input = ulib::read_stdin_char();

        if input == b'\r' || input == b'\n' {
            return len;
        }

        if len >= buffer.len() {
            continue;
        }

        buffer[len] = input;
        len += 1;

        let echo = [input];
        ulib::stdout(&echo);
    }
}
