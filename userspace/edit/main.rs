#![no_std]
#![no_main]

use core::{arch::global_asm, cell::UnsafeCell, panic::PanicInfo};

const FILE_CAPACITY: usize = 4 * 1024;
const PATH_CAPACITY: usize = 256;
const VIEW_ROWS: usize = 16;

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

struct FileBuffer(UnsafeCell<[u8; FILE_CAPACITY]>);

unsafe impl Sync for FileBuffer {}

static FILE_BUFFER: FileBuffer = FileBuffer(UnsafeCell::new([0; FILE_CAPACITY]));

struct Editor {
    fd: usize,
    len: usize,
    cursor: usize,
    first_visible_line: usize,
    preferred_column: Option<usize>,
    escape_state: u8,
    dirty: bool,
    message: &'static [u8],
}

#[no_mangle]
extern "C" fn rust_main() -> ! {
    let mut path = [0u8; PATH_CAPACITY];

    ulib::stdout(b"edit - simple text editor\nFile: ");
    let path_len = read_line(&mut path);
    if path_len == 0 {
        ulib::stdout(b"\nNo file selected.\n");
        ulib::exit();
    }

    let path = &path[..path_len];
    let fd = match open_or_create(path) {
        Some(fd) => fd,
        None => {
            ulib::stdout(b"\nCould not open or create file.\n");
            ulib::exit();
        }
    };

    let buffer = file_buffer();
    let len = ulib::read(fd, buffer);
    if len == FILE_CAPACITY {
        let mut extra = [0u8; 1];
        if ulib::read(fd, &mut extra) != 0 {
            ulib::close(fd);
            ulib::stdout(b"\nFile is larger than edit's 4 KiB buffer.\n");
            ulib::exit();
        }
    }

    let mut editor = Editor {
        fd: fd,
        len: len,
        cursor: 0,
        first_visible_line: 0,
        preferred_column: None,
        escape_state: 0,
        dirty: false,
        message: b"Arrows move  Esc-w save  Esc-q quit",
    };

    loop {
        editor.render(path, buffer);

        if editor.handle_input(ulib::read_stdin_char(), buffer) {
            break;
        }
    }

    ulib::close(fd);
    ulib::stdout(b"\x1B[2J\x1B[H");
    ulib::exit();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ulib::exit();
}

fn file_buffer() -> &'static mut [u8; FILE_CAPACITY] {
    unsafe { &mut *FILE_BUFFER.0.get() }
}

fn open_or_create(path: &[u8]) -> Option<usize> {
    let fd = ulib::open(path);
    if fd != 0 {
        return Some(fd);
    }

    let fd = ulib::create(path);
    (fd != 0).then_some(fd)
}

fn read_line(buffer: &mut [u8]) -> usize {
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

        if input.is_ascii_control() || len == buffer.len() {
            continue;
        }

        buffer[len] = input;
        len += 1;
        ulib::stdout(&[input]);
    }
}

impl Editor {
    fn handle_input(&mut self, input: u8, buffer: &mut [u8]) -> bool {
        if self.escape_state == 1 {
            if input == b'[' {
                self.escape_state = 2;
                return false;
            }

            self.escape_state = 0;
            match input {
                b'w' => self.save(buffer),
                b'q' => return true,
                _ => {}
            }

            return false;
        } else if self.escape_state == 2 {
            self.escape_state = 0;
            self.handle_arrow_key(input, buffer);
            return false;
        }

        if input == b'\x1B' {
            self.escape_state = 1;
            return false;
        }

        match input {
            b'\x08' | b'\x7F' => self.backspace(buffer),
            b'\r' | b'\n' => self.insert_byte(buffer, b'\n'),
            byte if !byte.is_ascii_control() || byte == b'\t' => self.insert_byte(buffer, byte),
            _ => {}
        }

        false
    }

    fn handle_arrow_key(&mut self, input: u8, buffer: &[u8]) {
        match input {
            b'A' => self.move_up(buffer),
            b'B' => self.move_down(buffer),
            b'C' => {
                if self.cursor < self.len {
                    self.cursor += 1;
                }

                self.preferred_column = None;
            }
            b'D' => {
                self.cursor = self.cursor.saturating_sub(1);
                self.preferred_column = None;
            }
            _ => {}
        }
    }

    fn insert_byte(&mut self, buffer: &mut [u8], byte: u8) {
        if self.len == buffer.len() {
            self.message = b"Buffer full";
            return;
        }

        buffer.copy_within(self.cursor..self.len, self.cursor + 1);
        buffer[self.cursor] = byte;
        self.cursor += 1;
        self.len += 1;
        self.preferred_column = None;
        self.dirty = true;
    }

    fn backspace(&mut self, buffer: &mut [u8]) {
        if self.cursor == 0 {
            return;
        }

        self.cursor -= 1;
        buffer.copy_within(self.cursor + 1..self.len, self.cursor);
        self.len -= 1;
        self.preferred_column = None;
        self.dirty = true;
    }

    fn move_up(&mut self, buffer: &[u8]) {
        let line_start = self.line_start(buffer, self.cursor);
        if line_start == 0 {
            return;
        }

        let column = self.preferred_column.unwrap_or(self.cursor - line_start);
        let previous_end = line_start - 1;
        let previous_start = self.line_start(buffer, previous_end);
        self.cursor = core::cmp::min(previous_start + column, previous_end);
        self.preferred_column = Some(column);
    }

    fn move_down(&mut self, buffer: &[u8]) {
        let line_start = self.line_start(buffer, self.cursor);
        let column = self.preferred_column.unwrap_or(self.cursor - line_start);
        let line_end = self.line_end(buffer, self.cursor);
        if line_end == self.len {
            return;
        }

        let next_start = line_end + 1;
        let next_end = self.line_end(buffer, next_start);
        self.cursor = core::cmp::min(next_start + column, next_end);
        self.preferred_column = Some(column);
    }

    fn save(&mut self, buffer: &[u8]) {
        if !ulib::truncate(self.fd, 0) {
            self.message = b"Could not truncate file";
            return;
        }

        if ulib::write_file(self.fd, &buffer[..self.len]) != self.len {
            self.message = b"Could not write file";
            return;
        }

        self.dirty = false;
        self.message = b"Written";
    }

    fn line_start(&self, buffer: &[u8], mut index: usize) -> usize {
        index = core::cmp::min(index, self.len);
        while index > 0 && buffer[index - 1] != b'\n' {
            index -= 1;
        }

        index
    }

    fn line_end(&self, buffer: &[u8], mut index: usize) -> usize {
        index = core::cmp::min(index, self.len);
        while index < self.len && buffer[index] != b'\n' {
            index += 1;
        }

        index
    }

    fn cursor_line(&self, buffer: &[u8]) -> usize {
        buffer[..self.cursor]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    }

    fn keep_cursor_visible(&mut self, buffer: &[u8]) {
        let cursor_line = self.cursor_line(buffer);
        if cursor_line < self.first_visible_line {
            self.first_visible_line = cursor_line;
        } else if cursor_line >= self.first_visible_line + VIEW_ROWS {
            self.first_visible_line = cursor_line - VIEW_ROWS + 1;
        }
    }

    fn render(&mut self, path: &[u8], buffer: &[u8]) {
        self.keep_cursor_visible(buffer);
        ulib::stdout(b"\x1B[2J\x1B[H\x1B[7m edit ");
        ulib::stdout(path);
        ulib::stdout(b" \x1B[0m\x1B[K\n");

        let mut line = 0;
        let mut index = 0;
        while line < self.first_visible_line && index < self.len {
            index = self.line_end(buffer, index);
            if index < self.len {
                index += 1;
            }

            line += 1;
        }

        for _ in 0..VIEW_ROWS {
            if index < self.len {
                let end = self.line_end(buffer, index);
                ulib::stdout(b"  ");
                ulib::stdout(&buffer[index..end]);
                ulib::stdout(b"\x1B[K\n");
                index = if end < self.len { end + 1 } else { self.len };
            } else {
                ulib::stdout(b"~\x1B[K\n");
            }
        }

        ulib::stdout(b"\x1B[7m ");
        if self.dirty {
            ulib::stdout(b"modified ");
        }

        ulib::stdout(self.message);
        ulib::stdout(b" \x1B[0m\x1B[K");

        let cursor_line = self.cursor_line(buffer);
        let cursor_column = self.cursor - self.line_start(buffer, self.cursor);
        self.move_terminal_cursor(
            2 + cursor_line.saturating_sub(self.first_visible_line),
            core::cmp::min(cursor_column + 3, 80),
        );
    }

    fn move_terminal_cursor(&self, row: usize, column: usize) {
        ulib::stdout(b"\x1B[");
        write_number(row);
        ulib::stdout(b";");
        write_number(column);
        ulib::stdout(b"H");
    }
}

fn write_number(mut number: usize) {
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

    for digit in digits[..len].iter().rev() {
        ulib::stdout(&[*digit]);
    }
}
