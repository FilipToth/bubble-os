#![no_std]

use core::arch::asm;

pub const STDIN: usize = 0;
pub const STDOUT: usize = 1;
pub const STDERR: usize = 2;

const SYS_EXIT: usize = 1;
const SYS_WRITE: usize = 2;
const SYS_READ: usize = 3;
const SYS_EXECUTE: usize = 4;
const SYS_YIELD: usize = 5;
const SYS_WAIT_FOR_PROCESS: usize = 6;
const SYS_READ_DIR: usize = 7;
const SYS_CD: usize = 8;
const SYS_OPEN: usize = 9;
const SYS_CLOSE: usize = 10;
const SYS_TRUNCATE: usize = 11;
const SYS_CREATE: usize = 12;
const SYS_MKDIR: usize = 13;
const SYS_UNLINK: usize = 14;
const SYS_RMDIR: usize = 15;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub name: [u8; 64],
    pub attr: u8,
    pub size: u32,
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0; 64],
            attr: 0,
            size: 0,
        }
    }
}

#[inline(always)]
unsafe fn syscall0(number: usize) -> usize {
    let ret: usize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
    );

    ret
}

#[inline(always)]
unsafe fn syscall1(number: usize, arg0: usize) -> usize {
    let ret: usize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
        in("rdi") arg0,
    );

    ret
}

#[inline(always)]
unsafe fn syscall2(number: usize, arg0: usize, arg1: usize) -> usize {
    let ret: usize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
        in("rdi") arg0,
        in("rsi") arg1,
    );

    ret
}

#[inline(always)]
unsafe fn syscall3(number: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let ret: usize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
    );

    ret
}

pub fn write(fd: usize, bytes: &[u8]) -> usize {
    unsafe { syscall3(SYS_WRITE, fd, bytes.as_ptr() as usize, bytes.len()) }
}

pub fn write_file(fd: usize, bytes: &[u8]) -> usize {
    write(fd, bytes)
}

pub fn write_existing_file(path: &[u8], bytes: &[u8]) -> bool {
    let fd = open(path);
    if fd == 0 {
        return false;
    }

    let bytes_written = write_file(fd, bytes);
    let truncated = truncate(fd, bytes_written);
    close(fd);

    bytes_written == bytes.len() && truncated
}

pub fn stdout(bytes: &[u8]) -> usize {
    write(STDOUT, bytes)
}

pub fn stderr(bytes: &[u8]) -> usize {
    write(STDERR, bytes)
}

pub fn read(fd: usize, buffer: &mut [u8]) -> usize {
    unsafe { syscall3(SYS_READ, fd, buffer.as_mut_ptr() as usize, buffer.len()) }
}

pub fn read_stdin_char() -> u8 {
    unsafe { syscall1(SYS_READ, STDIN) as u8 }
}

pub fn execute(path: &[u8]) -> usize {
    unsafe { syscall2(SYS_EXECUTE, path.as_ptr() as usize, path.len()) }
}

pub fn yield_now() {
    unsafe {
        syscall0(SYS_YIELD);
    }
}

pub fn wait_for_process(pid: usize) {
    unsafe {
        syscall1(SYS_WAIT_FOR_PROCESS, pid);
    }
}

pub fn read_dir(entries: &mut [DirEntry]) -> usize {
    unsafe { syscall2(SYS_READ_DIR, entries.as_mut_ptr() as usize, entries.len()) }
}

pub fn cd(path: &[u8]) -> bool {
    unsafe { syscall2(SYS_CD, path.as_ptr() as usize, path.len()) != 0 }
}

pub fn open(path: &[u8]) -> usize {
    unsafe { syscall2(SYS_OPEN, path.as_ptr() as usize, path.len()) }
}

pub fn create(path: &[u8]) -> usize {
    unsafe { syscall2(SYS_CREATE, path.as_ptr() as usize, path.len()) }
}

pub fn mkdir(path: &[u8]) -> bool {
    unsafe { syscall2(SYS_MKDIR, path.as_ptr() as usize, path.len()) != 0 }
}

pub fn unlink(path: &[u8]) -> bool {
    unsafe { syscall2(SYS_UNLINK, path.as_ptr() as usize, path.len()) != 0 }
}

pub fn rmdir(path: &[u8]) -> bool {
    unsafe { syscall2(SYS_RMDIR, path.as_ptr() as usize, path.len()) != 0 }
}

pub fn close(fd: usize) -> bool {
    unsafe { syscall1(SYS_CLOSE, fd) != 0 }
}

pub fn truncate(fd: usize, size: usize) -> bool {
    unsafe { syscall2(SYS_TRUNCATE, fd, size) != 0 }
}

pub fn exit() -> ! {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") SYS_EXIT,
            options(noreturn),
        );
    }
}
