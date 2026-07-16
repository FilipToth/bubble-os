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
    let mut input_buffer = [0u8; 256];
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

        if command.starts_with(b"write ") {
            let bytes = trim_ascii_spaces(&command[6..]);
            if ulib::write_existing_file(b"/res/resource.txt", bytes) {
                ulib::stdout(b"Wrote to res/resource.txt\n");
            } else {
                ulib::stdout(b"Failed to write to res/resource.txt\n");
            }

            continue;
        }

        if command.starts_with(b"touch ") {
            let path = trim_ascii_spaces(&command[6..]);
            let fd = ulib::create(path);
            if fd != 0 {
                ulib::close(fd);
                ulib::stdout(b"Created file\n");
            } else {
                ulib::stdout(b"Could not create file\n");
            }

            continue;
        }

        if command.starts_with(b"mkdir ") {
            let path = trim_ascii_spaces(&command[6..]);
            if ulib::mkdir(path) {
                ulib::stdout(b"Created directory\n");
            } else {
                ulib::stdout(b"Could not create directory\n");
            }

            continue;
        }

        if command.starts_with(b"unlink ") {
            let path = trim_ascii_spaces(&command[7..]);
            if ulib::unlink(path) {
                ulib::stdout(b"Removed file\n");
            } else {
                ulib::stdout(b"Could not remove file\n");
            }

            continue;
        }

        if command.starts_with(b"rmdir ") {
            let path = trim_ascii_spaces(&command[6..]);
            if ulib::rmdir(path) {
                ulib::stdout(b"Removed directory\n");
            } else {
                ulib::stdout(b"Could not remove directory\n");
            }

            continue;
        }

        if command == b"uptime" {
            let mut timespec = ulib::Timespec::zero();
            if ulib::clock_gettime(ulib::CLOCK_MONOTONIC, &mut timespec) {
                ulib::stdout(b"Up for ");
                print_number(timespec.tv_sec as usize);
                ulib::stdout(b"s\n");
            } else {
                ulib::stdout(b"Could not read the monotonic clock\n");
            }

            continue;
        }

        if command == b"date" {
            let unix_time = ulib::time();
            if unix_time != 0 {
                ulib::stdout(b"Unix time: ");
                print_number(unix_time as usize);
                ulib::stdout(b"\n");
            } else {
                ulib::stdout(b"Could not read the wall clock\n");
            }

            continue;
        }

        if command.starts_with(b"sleep ") {
            let argument = trim_ascii_spaces(&command[6..]);
            let slept = match parse_number(argument) {
                Some(seconds) => ulib::sleep(seconds as i64),
                None => false,
            };

            if !slept {
                ulib::stdout(b"Usage: sleep <seconds>\n");
            }

            continue;
        }

        if !launch(command) {
            ulib::stdout(b"Program or command not found...\n");
        }
    }
}

struct Cwd {
    path: [u8; 256],
    len: usize,
}

impl Cwd {
    const fn new() -> Self {
        Self {
            path: [0; 256],
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

        if input == b'\x08' || input == b'\x7F' {
            if len > 0 {
                len -= 1;
                ulib::stdout(b"\x08 \x08");
            }

            continue;
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

fn print_number(mut number: usize) {
    let mut digits = [0u8; 20];
    let mut len = 0;

    loop {
        digits[len] = b'0' + (number % 10) as u8;
        len += 1;
        number /= 10;

        if number == 0 {
            break;
        }
    }

    while len > 0 {
        len -= 1;
        ulib::stdout(&digits[len..len + 1]);
    }
}

fn parse_number(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }

    let mut number: usize = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }

        number = number.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }

    Some(number)
}

fn split_next_path_component(bytes: &[u8]) -> (&[u8], Option<&[u8]>) {
    match bytes.iter().position(|b| *b == b'/') {
        Some(index) => (&bytes[..index], Some(&bytes[index + 1..])),
        None => (bytes, None),
    }
}

fn launch(command: &[u8]) -> bool {
    let (program, args) = split_command_line(command);

    let pid = if program.contains(&b'/') {
        ulib::execute(program, args)
    } else {
        let bin_pid = execute_from_bin(program, args);
        if bin_pid == 0 {
            ulib::execute(program, args)
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

/// Splits a command line into the program name and its argument string.
fn split_command_line(command: &[u8]) -> (&[u8], &[u8]) {
    match command.iter().position(|byte| byte.is_ascii_whitespace()) {
        Some(index) => (
            &command[..index],
            trim_ascii_spaces(&command[index + 1..]),
        ),
        None => (command, b""),
    }
}

fn execute_from_bin(program: &[u8], args: &[u8]) -> usize {
    const BIN_PREFIX: &[u8] = b"/bin/";

    let mut path_buffer = [0u8; 261];
    let path_len = BIN_PREFIX.len() + program.len();
    if path_len > path_buffer.len() {
        return 0;
    }

    path_buffer[..BIN_PREFIX.len()].copy_from_slice(BIN_PREFIX);
    path_buffer[BIN_PREFIX.len()..path_len].copy_from_slice(program);

    ulib::execute(&path_buffer[..path_len], args)
}
