#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

// runs on the kernel-provided stack, with the System V argument frame at the
// initial stack pointer. Switching to a .bss stack here would throw away the
// frame, and with it the environment
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

/// Exit statuses at or above this mean the kernel killed the process after a
/// CPU fault, the rest of the value is the exception vector.
const FAULT_STATUS_BASE: usize = 128;

#[no_mangle]
extern "C" fn rust_main(_argc: usize, _argv: *const *const u8, envp: *const *const u8) -> ! {
    ulib::set_environ(envp);

    let mut input_buffer = [0u8; 256];
    let mut cwd = Cwd::new();
    let mut last_status = 0usize;

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

        // parse once, here. Builtins used to slice the raw line, so a quoted
        // argument reached them with its quotes still attached while external
        // programs got them stripped, and the two disagreed about what a file
        // was called
        let mut argv = ulib::ArgvBlob::new();
        if !ulib::parse_command_line(command, &mut argv) {
            ulib::stdout(b"Command line too long or too complex\n");
            continue;
        }

        let Some(name) = argv.entry(0) else {
            continue;
        };

        let argument = argv.entry(1);

        if name == b"cd" {
            let Some(path) = argument else {
                ulib::stdout(b"Usage: cd <path>\n");
                continue;
            };

            match ulib::cd(path) {
                Ok(()) => cwd.update(path),
                Err(error) => print_error(b"cd", error),
            }

            continue;
        }

        if name == b"write" {
            let mut text = [0u8; 256];
            let Some(len) = join_arguments(&argv, &mut text) else {
                ulib::stdout(b"Usage: write <text>\n");
                continue;
            };

            if ulib::write_existing_file(b"/res/resource.txt", &text[..len]) {
                ulib::stdout(b"Wrote to res/resource.txt\n");
            } else {
                ulib::stdout(b"Failed to write to res/resource.txt\n");
            }

            continue;
        }

        if name == b"touch" {
            let Some(path) = argument else {
                ulib::stdout(b"Usage: touch <path>\n");
                continue;
            };

            match ulib::create(path) {
                Ok(fd) => {
                    let _ = ulib::close(fd);
                    ulib::stdout(b"Created file\n");
                }
                Err(error) => print_error(b"touch", error),
            }

            continue;
        }

        if name == b"mkdir" {
            let Some(path) = argument else {
                ulib::stdout(b"Usage: mkdir <path>\n");
                continue;
            };

            match ulib::mkdir(path) {
                Ok(()) => {
                    ulib::stdout(b"Created directory\n");
                }
                Err(error) => print_error(b"mkdir", error),
            }

            continue;
        }

        if name == b"unlink" {
            let Some(path) = argument else {
                ulib::stdout(b"Usage: unlink <path>\n");
                continue;
            };

            match ulib::unlink(path) {
                Ok(()) => {
                    ulib::stdout(b"Removed file\n");
                }
                Err(error) => print_error(b"unlink", error),
            }

            continue;
        }

        if name == b"rmdir" {
            let Some(path) = argument else {
                ulib::stdout(b"Usage: rmdir <path>\n");
                continue;
            };

            match ulib::rmdir(path) {
                Ok(()) => {
                    ulib::stdout(b"Removed directory\n");
                }
                Err(error) => print_error(b"rmdir", error),
            }

            continue;
        }

        if name == b"uptime" {
            let mut timespec = ulib::Timespec::zero();
            if ulib::clock_gettime(ulib::CLOCK_MONOTONIC, &mut timespec).is_ok() {
                ulib::stdout(b"Up for ");
                print_number(timespec.tv_sec as usize);
                ulib::stdout(b"s\n");
            } else {
                ulib::stdout(b"Could not read the monotonic clock\n");
            }

            continue;
        }

        if name == b"date" {
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

        if name == b"sleep" {
            let slept = match argument.and_then(parse_number) {
                Some(seconds) => ulib::sleep(seconds as i64).is_ok(),
                None => false,
            };

            if !slept {
                ulib::stdout(b"Usage: sleep <seconds>\n");
            }

            continue;
        }

        if name == b"env" {
            for entry in ulib::environ() {
                ulib::stdout(entry);
                ulib::stdout(b"\n");
            }

            continue;
        }

        if name == b"pid" {
            match ulib::getpid() {
                Ok(pid) => {
                    print_number(pid);
                    ulib::stdout(b"\n");
                }
                Err(error) => print_error(b"pid", error),
            }

            continue;
        }

        if name == b"status" {
            print_number(last_status);
            ulib::stdout(b"\n");
            continue;
        }

        launch(&argv, &mut last_status);
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
        let path = ulib::trim_ascii_spaces(path);
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
    ulib::exit(101);
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

fn launch(argv: &ulib::ArgvBlob, last_status: &mut usize) {
    let status = match ulib::run(argv) {
        Ok(status) => status,
        Err(ulib::Errno::NOENT) => {
            ulib::stdout(b"Program or command not found...\n");
            return;
        }
        Err(error) => {
            print_error(argv.program_name(), error);
            return;
        }
    };

    *last_status = status;

    // a program the kernel killed is easy to miss otherwise, it dies
    // wherever it was and the shell just prints the next prompt
    if status >= FAULT_STATUS_BASE {
        ulib::stdout(b"Killed by fault, exception vector ");
        print_number(status - FAULT_STATUS_BASE);
        ulib::stdout(b"\n");
    }
}

/// Joins the arguments after the command name with single spaces.
///
/// The `write` builtin takes free text rather than a path, so it wants the
/// words back together. Quoting still works, `write 'a  b'` keeps its spacing
/// because that was one argument.
///
/// ## Returns
/// The number of bytes written, or `None` when there were no arguments or
/// they did not fit.
fn join_arguments(argv: &ulib::ArgvBlob, buffer: &mut [u8]) -> Option<usize> {
    let mut len = 0;

    for argument in argv.iter().skip(1) {
        if len > 0 {
            if len == buffer.len() {
                return None;
            }

            buffer[len] = b' ';
            len += 1;
        }

        let end = len.checked_add(argument.len())?;
        if end > buffer.len() {
            return None;
        }

        buffer[len..end].copy_from_slice(argument);
        len = end;
    }

    if len == 0 {
        return None;
    }

    Some(len)
}

/// Prints a failed command as `name: reason`.
///
/// ## Arguments
///
/// - `name` what was being attempted
/// - `error` the error the kernel reported
fn print_error(name: &[u8], error: ulib::Errno) {
    ulib::stdout(name);
    ulib::stdout(b": ");
    ulib::stdout(error.as_str().as_bytes());
    ulib::stdout(b"\n");
}

