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
const SYS_BRK: usize = 18;
const SYS_SBRK: usize = 19;

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

        Some(unsafe { cstr_bytes(arg) })
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

/// The bytes of a NUL-terminated string, without the terminator.
///
/// ## Arguments
///
/// - `ptr` a pointer to the first byte of the string
unsafe fn cstr_bytes(ptr: *const u8) -> &'static [u8] {
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }

    core::slice::from_raw_parts(ptr, len)
}

/// The process environment, read from the System V style entry stack frame.
///
/// The pointer array is NULL terminated rather than counted, which is the one
/// thing that keeps this from being [`Args`].
#[derive(Clone, Copy)]
pub struct Env {
    envp: *const *const u8,
    index: usize,
}

impl Env {
    /// ## Arguments
    ///
    /// - `envp` the environment pointer array from the entry stack
    pub fn new(envp: *const *const u8) -> Self {
        Self {
            envp: envp,
            index: 0,
        }
    }

    /// The entry at an index as a `KEY=VALUE` byte slice, or `None` at or
    /// past the end of the array.
    pub fn get(&self, index: usize) -> Option<&'static [u8]> {
        if self.envp.is_null() {
            return None;
        }

        let entry = unsafe { *self.envp.add(index) };
        if entry.is_null() {
            return None;
        }

        Some(unsafe { cstr_bytes(entry) })
    }
}

impl Iterator for Env {
    type Item = &'static [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.get(self.index)?;
        self.index += 1;

        Some(entry)
    }
}

/// The environment the process was started with, as handed to `_start`.
static mut ENVIRON: *const *const u8 = core::ptr::null();

/// Records the environment pointer so [`getenv`] and [`system`] can be free
/// functions instead of taking an [`Env`] everywhere.
///
/// Call it first thing in `rust_main`, with the third argument `_start` takes
/// off the entry stack. A program that never calls it simply has no
/// environment, and [`getenv`] returns `None` for every key.
///
/// ## Arguments
///
/// - `envp` the environment pointer array from the entry stack
pub fn set_environ(envp: *const *const u8) {
    unsafe { ENVIRON = envp };
}

/// The process environment.
pub fn environ() -> Env {
    Env::new(unsafe { ENVIRON })
}

/// Looks up an environment variable.
///
/// ## Arguments
///
/// - `key` the variable name, without the `=`
///
/// ## Returns
/// The value, or `None` when the variable is not set.
pub fn getenv(key: &[u8]) -> Option<&'static [u8]> {
    for entry in environ() {
        let Some(rest) = entry.strip_prefix(key) else {
            continue;
        };

        if rest.first() == Some(&b'=') {
            return Some(&rest[1..]);
        }
    }

    None
}

/// Where programs are looked up when the environment has no `PATH`.
const DEFAULT_PATH: &[u8] = b"/bin";

/// Maximum length of a program path built during a `PATH` search.
const PATH_MAX: usize = 512;

/// Runs a command and waits for it to finish.
///
/// The command is split on the first whitespace into a program and its
/// argument string, so a multi-word argument cannot survive. That is the same
/// limitation the execute syscall itself has.
///
/// ## Arguments
///
/// - `command` the command line to run
///
/// ## Returns
/// The exit status of the program, or `None` when nothing could be launched.
pub fn system(command: &[u8]) -> Option<usize> {
    let (program, args) = split_command_line(command);
    if program.is_empty() {
        return None;
    }

    let pid = spawn(program, args)?;
    Some(wait_for_process(pid))
}

/// Launches a program without waiting for it.
///
/// A name containing `/` is taken as a path and used as it is. A bare name is
/// looked up in each colon-separated entry of `PATH`, and finally relative to
/// the working directory.
///
/// ## Arguments
///
/// - `program` the program name or path
/// - `args` the whitespace-separated argument string
///
/// ## Returns
/// The new pid, or `None` when no candidate could be launched.
pub fn spawn(program: &[u8], args: &[u8]) -> Option<usize> {
    if program.contains(&b'/') {
        return match execute(program, args) {
            0 => None,
            pid => Some(pid),
        };
    }

    let path = getenv(b"PATH").unwrap_or(DEFAULT_PATH);
    for directory in path.split(|byte| *byte == b':') {
        let mut buffer = [0u8; PATH_MAX];
        let Some(candidate) = join_path(&mut buffer, directory, program) else {
            continue;
        };

        let pid = execute(candidate, args);
        if pid != 0 {
            return Some(pid);
        }
    }

    // nothing on the search path, try the working directory
    match execute(program, args) {
        0 => None,
        pid => Some(pid),
    }
}

/// Splits a command line into the program name and its argument string.
///
/// ## Arguments
///
/// - `command` the command line to split
pub fn split_command_line(command: &[u8]) -> (&[u8], &[u8]) {
    match command.iter().position(|byte| byte.is_ascii_whitespace()) {
        Some(index) => (&command[..index], trim_ascii_spaces(&command[index + 1..])),
        None => (command, b""),
    }
}

/// Strips leading and trailing ASCII whitespace.
///
/// ## Arguments
///
/// - `bytes` the slice to trim
pub fn trim_ascii_spaces(mut bytes: &[u8]) -> &[u8] {
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

/// Joins a directory and a program name into a caller-provided buffer.
///
/// ## Arguments
///
/// - `buffer` scratch space for the joined path
/// - `directory` the search path entry
/// - `program` the program name
///
/// ## Returns
/// The joined path, or `None` when it does not fit into the buffer.
fn join_path<'a>(buffer: &'a mut [u8], directory: &[u8], program: &[u8]) -> Option<&'a [u8]> {
    let separator: &[u8] = if directory.is_empty() || directory.ends_with(b"/") {
        b""
    } else {
        b"/"
    };

    let len = directory.len() + separator.len() + program.len();
    if len > buffer.len() {
        return None;
    }

    let mut offset = 0;
    for part in [directory, separator, program] {
        buffer[offset..offset + part.len()].copy_from_slice(part);
        offset += part.len();
    }

    Some(&buffer[..len])
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

/// Waits for a process to exit.
///
/// ## Arguments
///
/// - `pid` the pid to wait for
///
/// ## Returns
/// The status the process exited with. A status of 128 or more means the
/// kernel killed it after a CPU fault, and the value is 128 plus the
/// exception vector.
pub fn wait_for_process(pid: usize) -> usize {
    unsafe { syscall1(SYS_WAIT_FOR_PROCESS, pid) }
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

/// Moves the program break to an absolute address.
///
/// The heap starts one page past the end of the program image, so the only way
/// to learn a usable address is to ask for the current break with `sbrk(0)`
/// first. Lowering the break gives pages back and drops whatever was in them.
///
/// ## Arguments
///
/// - `end_data_segment` the requested break
///
/// ## Returns
/// `0` on success and `-1` on failure, as `int brk(void *)` does.
pub fn brk(end_data_segment: usize) -> i32 {
    // the kernel answers with the new break, and with zero on failure
    let result = unsafe { syscall1(SYS_BRK, end_data_segment) };
    if result == 0 {
        -1
    } else {
        0
    }
}

/// Moves the program break by a signed number of bytes.
///
/// `sbrk(0)` reads the current break without moving it, which is how a program
/// finds where its heap begins.
///
/// ## Arguments
///
/// - `increment` how far to move the break, negative to give memory back
///
/// ## Returns
/// The break as it was before the call, so the return value of a positive
/// increment is the start of the newly usable bytes. `None` on failure.
pub fn sbrk(increment: isize) -> Option<usize> {
    let result = unsafe { syscall1(SYS_SBRK, increment as usize) };
    if result == 0 {
        return None;
    }

    // the kernel returns the new break, sbrk is defined to hand back the old
    Some((result as isize - increment) as usize)
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

/// Ends the current process.
///
/// ## Arguments
///
/// - `status` the exit status, 0 means success. Keep it below 128, the
///   kernel reserves that range for processes it kills after a CPU fault
pub fn exit(status: usize) -> ! {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") SYS_EXIT,
            in("rdi") status,
            options(noreturn),
        );
    }
}
