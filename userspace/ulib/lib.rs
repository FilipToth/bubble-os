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
const SYS_CLOCK_GETTIME: usize = 16;
const SYS_NANOSLEEP: usize = 17;

pub const CLOCK_REALTIME: usize = 0;
pub const CLOCK_MONOTONIC: usize = 1;

pub const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;
pub const NANOSECONDS_PER_MILLISECOND: i64 = 1_000_000;

/// A point in time, laid out like the POSIX `timespec`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Timespec {
    pub const fn zero() -> Self {
        Self {
            tv_sec: 0,
            tv_nsec: 0,
        }
    }

    pub const fn from_milliseconds(milliseconds: i64) -> Self {
        Self {
            tv_sec: milliseconds / 1_000,
            tv_nsec: (milliseconds % 1_000) * NANOSECONDS_PER_MILLISECOND,
        }
    }
}

/// Maximum filename bytes in a [`DirEntry`]; must match the kernel's
/// `SyscallDirEntry` layout.
pub const DIR_ENTRY_NAME_CAPACITY: usize = 256;

/// Directory entry attribute flag marking a subdirectory.
pub const DIR_ENTRY_ATTR_DIRECTORY: u8 = 0x10;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub name: [u8; DIR_ENTRY_NAME_CAPACITY],
    pub attr: u8,
    pub size: u32,
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0; DIR_ENTRY_NAME_CAPACITY],
            attr: 0,
            size: 0,
        }
    }

    pub fn is_directory(&self) -> bool {
        self.attr & DIR_ENTRY_ATTR_DIRECTORY != 0
    }

    /// The entry name as a byte slice, without trailing NUL padding.
    pub fn name_bytes(&self) -> &[u8] {
        let len = self
            .name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.name.len());

        &self.name[..len]
    }
}

/// The process arguments, read from the System V style entry stack frame.
///
/// Construct one in `rust_main` from the `argc`/`argv` values that `_start`
/// takes off the initial stack pointer.
#[derive(Clone, Copy)]
pub struct Args {
    argc: usize,
    argv: *const *const u8,
    index: usize,
}

impl Args {
    /// ## Arguments
    ///
    /// - `argc` the argument count from the entry stack
    /// - `argv` the argument pointer array from the entry stack
    pub fn new(argc: usize, argv: *const *const u8) -> Self {
        Self {
            argc: argc,
            argv: argv,
            index: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.argc
    }

    pub fn is_empty(&self) -> bool {
        self.argc == 0
    }

    /// The argument at an index as a byte slice, without the
    /// NUL terminator.
    pub fn get(&self, index: usize) -> Option<&'static [u8]> {
        if index >= self.argc {
            return None;
        }

        let arg = unsafe { *self.argv.add(index) };
        if arg.is_null() {
            return None;
        }

        let mut len = 0;
        while unsafe { *arg.add(len) } != 0 {
            len += 1;
        }

        Some(unsafe { core::slice::from_raw_parts(arg, len) })
    }
}

impl Iterator for Args {
    type Item = &'static [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let arg = self.get(self.index)?;
        self.index += 1;

        Some(arg)
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

#[inline(always)]
unsafe fn syscall4(number: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize) -> usize {
    let ret: usize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        in("r10") arg3,
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

/// Launches an ELF binary.
///
/// ## Arguments
///
/// - `path` the path of the binary
/// - `args` a whitespace-separated argument string; the kernel splits it
///   into `argv[1..]`, with the path becoming `argv[0]`
///
/// ## Returns
/// The new process PID, or 0 on failure.
pub fn execute(path: &[u8], args: &[u8]) -> usize {
    unsafe {
        syscall4(
            SYS_EXECUTE,
            path.as_ptr() as usize,
            path.len(),
            args.as_ptr() as usize,
            args.len(),
        )
    }
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

pub fn clock_gettime(clock_id: usize, timespec: &mut Timespec) -> bool {
    unsafe {
        syscall2(
            SYS_CLOCK_GETTIME,
            clock_id,
            timespec as *mut Timespec as usize,
        ) != 0
    }
}

/// Seconds since the Unix epoch, or 0 when the clock is unavailable.
pub fn time() -> i64 {
    let mut timespec = Timespec::zero();
    if !clock_gettime(CLOCK_REALTIME, &mut timespec) {
        return 0;
    }

    timespec.tv_sec
}

/// Nanoseconds since boot, or 0 when the clock is unavailable.
pub fn monotonic_ns() -> i64 {
    let mut timespec = Timespec::zero();
    if !clock_gettime(CLOCK_MONOTONIC, &mut timespec) {
        return 0;
    }

    timespec.tv_sec * NANOSECONDS_PER_SECOND + timespec.tv_nsec
}

pub fn nanosleep(duration: &Timespec) -> bool {
    unsafe { syscall1(SYS_NANOSLEEP, duration as *const Timespec as usize) != 0 }
}

pub fn sleep_ms(milliseconds: i64) -> bool {
    let duration = Timespec::from_milliseconds(milliseconds);
    nanosleep(&duration)
}

pub fn sleep(seconds: i64) -> bool {
    sleep_ms(seconds * 1_000)
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
