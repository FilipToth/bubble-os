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
    let mut cwd = Cwd::new();

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
        cwd.print_prompt();

        let input_len = read_command(&mut input_buffer);
        ulib::stdout(b"\n");

        if input_len == 0 {
            continue;
        }

        let command = &input_buffer[..input_len];
        if command.starts_with(b"cd ") {
            let path = trim_ascii_spaces(&command[3..]);
            if ulib::cd(path) {
                cwd.update(path);
            }

            continue;
        }

        if !launch(command) {
            ulib::stdout(b"Program or command not found...\n");
        }
    }
}

struct Cwd {
    path: [u8; 128],
    len: usize,
}

impl Cwd {
    const fn new() -> Self {
        Self {
            path: [0; 128],
            len: 0,
        }
    }

    fn print_prompt(&self) {
        ulib::stdout(b"~");

        if self.len > 0 {
            ulib::stdout(b"/");
            ulib::stdout(&self.path[..self.len]);
        }

        ulib::stdout(b" $ ");
    }

    fn update(&mut self, path: &[u8]) {
        let path = trim_ascii_spaces(path);
        if path == b"/" || path == b"~" {
            self.len = 0;
            return;
        }

        if path.starts_with(b"/") {
            self.len = 0;
        }

        let mut remaining = path;
        loop {
            let (component, rest) = split_next_path_component(remaining);
            self.apply_component(component);

            let Some(rest) = rest else {
                break;
            };

            remaining = rest;
        }
    }

    fn apply_component(&mut self, component: &[u8]) {
        if component.is_empty() || component == b"." {
            return;
        }

        if component == b".." {
            self.pop_component();
            return;
        }

        self.push_component(component);
    }

    fn push_component(&mut self, component: &[u8]) {
        let separator_len = if self.len == 0 { 0 } else { 1 };
        let new_len = self.len + separator_len + component.len();
        if new_len > self.path.len() {
            return;
        }

        if separator_len == 1 {
            self.path[self.len] = b'/';
            self.len += 1;
        }

        self.path[self.len..new_len].copy_from_slice(component);
        self.len = new_len;
    }

    fn pop_component(&mut self) {
        while self.len > 0 && self.path[self.len - 1] != b'/' {
            self.len -= 1;
        }

        if self.len > 0 {
            self.len -= 1;
        }
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

fn trim_ascii_spaces(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if !first.is_ascii_whitespace() {
            break;
        }

        bytes = rest;
    }

    while let Some((last, rest)) = bytes.split_last() {
        if !last.is_ascii_whitespace() {
            break;
        }

        bytes = rest;
    }

    bytes
}

fn split_next_path_component(bytes: &[u8]) -> (&[u8], Option<&[u8]>) {
    match bytes.iter().position(|b| *b == b'/') {
        Some(index) => (&bytes[..index], Some(&bytes[index + 1..])),
        None => (bytes, None),
    }
}

fn launch(command: &[u8]) -> bool {
    let pid = if command.contains(&b'/') {
        ulib::execute(command)
    } else {
        let bin_pid = execute_from_bin(command);
        if bin_pid == 0 {
            ulib::execute(command)
        } else {
            bin_pid
        }
    };

    if pid == 0 {
        return false;
    }

    ulib::wait_for_process(pid);
    true
}

fn execute_from_bin(command: &[u8]) -> usize {
    const BIN_PREFIX: &[u8] = b"/bin/";

    let mut path_buffer = [0u8; 132];
    let path_len = BIN_PREFIX.len() + command.len();
    if path_len > path_buffer.len() {
        return 0;
    }

    path_buffer[..BIN_PREFIX.len()].copy_from_slice(BIN_PREFIX);
    path_buffer[BIN_PREFIX.len()..path_len].copy_from_slice(command);

    ulib::execute(&path_buffer[..path_len])
}
