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
// 12 was create, folded into open as O_CREAT
const SYS_MKDIR: usize = 13;
const SYS_UNLINK: usize = 14;
const SYS_RMDIR: usize = 15;
const SYS_CLOCK_GETTIME: usize = 16;
const SYS_NANOSLEEP: usize = 17;
const SYS_BRK: usize = 18;
const SYS_SBRK: usize = 19;
const SYS_LSEEK: usize = 20;
const SYS_FSTAT: usize = 21;
const SYS_STAT: usize = 22;
const SYS_GETPID: usize = 23;

/// Maximum bytes an argument blob may occupy, matching the kernel's limit.
pub const ARGV_MAX_BYTES: usize = 4096;

/// Maximum number of arguments, including `argv[0]`.
pub const ARGV_MAX_COUNT: usize = 64;

/// `lseek` whence: the offset is absolute.
pub const SEEK_SET: usize = 0;

/// `lseek` whence: the offset is relative to the current position.
pub const SEEK_CUR: usize = 1;

/// `lseek` whence: the offset is relative to the end of the file.
pub const SEEK_END: usize = 2;

/// The largest error number a syscall return can carry.
const MAX_ERRNO: usize = 4095;

/// An error number returned by a syscall.
///
/// The kernel answers failures with the errno negated, so anything from `-1`
/// to `-MAX_ERRNO` is an error and everything else is a success value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Errno(pub usize);

impl Errno {
    pub const PERM: Errno = Errno(1);
    pub const NOENT: Errno = Errno(2);
    pub const SRCH: Errno = Errno(3);
    pub const IO: Errno = Errno(5);
    pub const NOEXEC: Errno = Errno(8);
    pub const BADF: Errno = Errno(9);
    pub const CHILD: Errno = Errno(10);
    pub const NOMEM: Errno = Errno(12);
    pub const ACCESS: Errno = Errno(13);
    pub const FAULT: Errno = Errno(14);
    pub const EXIST: Errno = Errno(17);
    pub const NOTDIR: Errno = Errno(20);
    pub const ISDIR: Errno = Errno(21);
    pub const INVAL: Errno = Errno(22);
    pub const MFILE: Errno = Errno(24);
    pub const NOSPC: Errno = Errno(28);
    pub const RANGE: Errno = Errno(34);
    pub const NOSYS: Errno = Errno(38);
    pub const NOTEMPTY: Errno = Errno(39);

    /// A short human readable name, for programs that report errors.
    pub fn as_str(&self) -> &'static str {
        match *self {
            Errno::PERM => "operation not permitted",
            Errno::NOENT => "no such file or directory",
            Errno::SRCH => "no such process",
            Errno::IO => "input/output error",
            Errno::NOEXEC => "bad executable format",
            Errno::BADF => "bad file descriptor",
            Errno::CHILD => "no child processes",
            Errno::NOMEM => "out of memory",
            Errno::ACCESS => "permission denied",
            Errno::FAULT => "bad address",
            Errno::EXIST => "file exists",
            Errno::NOTDIR => "not a directory",
            Errno::ISDIR => "is a directory",
            Errno::INVAL => "invalid argument",
            Errno::MFILE => "too many open files",
            Errno::NOSPC => "no space left on device",
            Errno::RANGE => "result out of range",
            Errno::NOSYS => "function not implemented",
            Errno::NOTEMPTY => "directory not empty",
            _ => "unknown error",
        }
    }
}

/// What every syscall wrapper returns.
pub type Result<T> = core::result::Result<T, Errno>;

/// Splits a raw syscall return into a success value or an error number.
///
/// ## Arguments
///
/// - `raw` the value the kernel left in `rax`
fn decode(raw: usize) -> Result<usize> {
    let signed = raw as isize;
    if signed < 0 && signed >= -(MAX_ERRNO as isize) {
        return Err(Errno(signed.unsigned_abs()));
    }

    Ok(raw)
}

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
/// The exit status of the program, or the error that stopped it launching.
pub fn system(command: &[u8]) -> Result<usize> {
    let mut argv = ArgvBlob::new();
    if !parse_command_line(command, &mut argv) {
        return Err(Errno::INVAL);
    }

    run(&argv)
}

/// Runs an already parsed argument vector and waits for it to finish.
///
/// Lets a caller that has parsed a command line for its own reasons, like a
/// shell checking for builtins, avoid parsing it a second time.
///
/// ## Arguments
///
/// - `argv` the argument vector, with the program name at index 0
///
/// ## Returns
/// The exit status of the program.
pub fn run(argv: &ArgvBlob) -> Result<usize> {
    // argv[0] is the name as it was typed, which is also what gets resolved
    // against PATH. The child still sees the typed name, not the resolved
    // path, the way exec does it
    let mut program = [0u8; PATH_MAX];
    let name = argv.program_name();
    if name.is_empty() || name.len() > program.len() {
        return Err(Errno::INVAL);
    }

    program[..name.len()].copy_from_slice(name);
    let program = &program[..name.len()];

    let pid = spawn(program, argv)?;
    wait_for_process(pid)
}

/// Splits a command line into arguments, honouring quotes.
///
/// A single or double quoted run is one argument no matter what whitespace it
/// contains, and the quotes themselves are not part of it. A backslash escapes
/// the next byte. This is only possible because argv reaches the kernel as a
/// blob; when it was a whitespace-joined string every one of these collapsed.
///
/// ## Arguments
///
/// - `command` the raw command line
/// - `argv` the blob the arguments are written into
///
/// ## Returns
/// Whether the line parsed and fit. An unterminated quote is still accepted,
/// closing at the end of the line, which is what an interactive shell does.
pub fn parse_command_line(command: &[u8], argv: &mut ArgvBlob) -> bool {
    let mut index = 0;
    let mut quote: Option<u8> = None;
    let mut in_argument = false;

    // starts the argument being built if one is not already open
    macro_rules! open_argument {
        () => {
            if !in_argument {
                if !argv.start_argument() {
                    return false;
                }

                in_argument = true;
            }
        };
    }

    while index < command.len() {
        let byte = command[index];
        index += 1;

        if let Some(closing) = quote {
            if byte == closing {
                quote = None;
            } else {
                open_argument!();
                if !argv.push_byte(byte) {
                    return false;
                }
            }

            continue;
        }

        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);

            // an empty quoted string is still an argument, so open one here
            // rather than waiting for a byte that may never come
            open_argument!();
            continue;
        }

        if byte.is_ascii_whitespace() {
            if in_argument {
                if !argv.finish_argument() {
                    return false;
                }

                in_argument = false;
            }

            continue;
        }

        // a backslash takes the next byte literally, including a quote, a
        // space, or another backslash
        let byte = if byte == b'\\' && index < command.len() {
            let escaped = command[index];
            index += 1;
            escaped
        } else {
            byte
        };

        open_argument!();
        if !argv.push_byte(byte) {
            return false;
        }
    }

    // an unterminated quote closes at the end of the line, which is what an
    // interactive shell does rather than rejecting the whole command
    if in_argument && !argv.finish_argument() {
        return false;
    }

    true
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
/// The new pid, or the error that stopped it launching.
pub fn spawn(program: &[u8], argv: &ArgvBlob) -> Result<usize> {
    if program.contains(&b'/') {
        return execute(program, argv);
    }

    let path = getenv(b"PATH").unwrap_or(DEFAULT_PATH);
    let mut last_error = Errno::NOENT;

    for directory in path.split(|byte| *byte == b':') {
        let mut buffer = [0u8; PATH_MAX];
        let Some(candidate) = join_path(&mut buffer, directory, program) else {
            continue;
        };

        match execute(candidate, argv) {
            Ok(pid) => return Ok(pid),

            // a missing candidate just means the next directory gets a turn,
            // anything else is worth reporting rather than walking past
            Err(Errno::NOENT) => continue,
            Err(error) => last_error = error,
        }
    }

    // nothing on the search path, try the working directory
    execute(program, argv).map_err(|error| match error {
        Errno::NOENT => last_error,
        error => error,
    })
}

/// The pid of the calling process.
pub fn getpid() -> Result<usize> {
    decode(unsafe { syscall0(SYS_GETPID) })
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

#[inline(always)]
unsafe fn syscall5(
    number: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
) -> usize {
    let ret: usize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        in("r10") arg3,
        in("r8") arg4,
    );

    ret
}

pub fn write(fd: usize, bytes: &[u8]) -> Result<usize> {
    decode(unsafe { syscall3(SYS_WRITE, fd, bytes.as_ptr() as usize, bytes.len()) })
}

pub fn write_file(fd: usize, bytes: &[u8]) -> Result<usize> {
    write(fd, bytes)
}

pub fn write_existing_file(path: &[u8], bytes: &[u8]) -> bool {
    let Ok(fd) = open(path, O_WRONLY) else {
        return false;
    };

    let bytes_written = write_file(fd, bytes).unwrap_or(0);
    let truncated = truncate(fd, bytes_written).is_ok();
    let _ = close(fd);

    bytes_written == bytes.len() && truncated
}

/// Writes to stdout, ignoring any error.
///
/// Printing is the last thing a program does on most error paths, so a
/// checked write there would just push the problem up with nowhere to go.
pub fn stdout(bytes: &[u8]) -> usize {
    write(STDOUT, bytes).unwrap_or(0)
}

/// Writes to stderr, ignoring any error.
pub fn stderr(bytes: &[u8]) -> usize {
    write(STDERR, bytes).unwrap_or(0)
}

/// Reads into a buffer.
///
/// ## Returns
/// The number of bytes read, where `0` means end of file.
pub fn read(fd: usize, buffer: &mut [u8]) -> Result<usize> {
    decode(unsafe { syscall3(SYS_READ, fd, buffer.as_mut_ptr() as usize, buffer.len()) })
}

/// Blocks until a key is pressed and returns it.
///
/// The keyboard handler writes the character straight into the waiting
/// process' `rax`, so this never carries an error number, but it is decoded
/// like any other return in case that path ever does start failing.
pub fn read_stdin_char() -> u8 {
    decode(unsafe { syscall1(SYS_READ, STDIN) }).unwrap_or(0) as u8
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
/// The new process PID, which is always at least 1.
pub fn execute(path: &[u8], argv: &ArgvBlob) -> Result<usize> {
    decode(unsafe {
        syscall5(
            SYS_EXECUTE,
            path.as_ptr() as usize,
            path.len(),
            argv.bytes().as_ptr() as usize,
            argv.bytes().len(),
            argv.count(),
        )
    })
}

/// A packed argument vector, ready to hand to [`execute`].
///
/// The kernel takes argv as NUL-separated bytes plus a count rather than as a
/// single string, so an argument may contain spaces or quotes. Building it
/// here keeps the packing in one place.
pub struct ArgvBlob {
    buffer: [u8; ARGV_MAX_BYTES],
    len: usize,
    count: usize,
}

impl ArgvBlob {
    pub const fn new() -> Self {
        Self {
            buffer: [0; ARGV_MAX_BYTES],
            len: 0,
            count: 0,
        }
    }

    /// The packed bytes, every entry NUL-terminated.
    pub fn bytes(&self) -> &[u8] {
        &self.buffer[..self.len]
    }

    /// How many entries the blob holds.
    pub fn count(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Appends one argument.
    ///
    /// ## Returns
    /// Whether it fit, both in bytes and against the argument count limit.
    pub fn push(&mut self, argument: &[u8]) -> bool {
        if self.count == ARGV_MAX_COUNT {
            return false;
        }

        // an argument may not contain a NUL, it is the separator
        if argument.contains(&0) {
            return false;
        }

        let end = self.len + argument.len() + 1;
        if end > self.buffer.len() {
            return false;
        }

        self.buffer[self.len..end - 1].copy_from_slice(argument);
        self.buffer[end - 1] = 0;
        self.len = end;
        self.count += 1;
        true
    }

    /// The first entry, which is the program name the child will see.
    pub fn program_name(&self) -> &[u8] {
        self.entry(0).unwrap_or(&[])
    }

    /// One entry by index, without its NUL terminator.
    pub fn entry(&self, index: usize) -> Option<&[u8]> {
        if index >= self.count {
            return None;
        }

        self.iter().nth(index)
    }

    /// Every entry in order.
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        // the trailing NUL of the last entry would otherwise yield an extra
        // empty slice after it
        self.buffer[..self.len]
            .split(|byte| *byte == 0)
            .take(self.count)
    }

    /// Begins an argument that is appended to a byte at a time.
    ///
    /// Lets a tokenizer build arguments straight into the blob instead of
    /// assembling each one in a buffer of its own first.
    pub fn start_argument(&mut self) -> bool {
        self.count < ARGV_MAX_COUNT && self.len < self.buffer.len()
    }

    /// Appends one byte to the argument being built.
    pub fn push_byte(&mut self, byte: u8) -> bool {
        // a NUL would end the argument early, and the caller cannot mean it
        if byte == 0 || self.len + 1 >= self.buffer.len() {
            return false;
        }

        self.buffer[self.len] = byte;
        self.len += 1;
        true
    }

    /// Terminates the argument being built and counts it.
    pub fn finish_argument(&mut self) -> bool {
        if self.count == ARGV_MAX_COUNT || self.len >= self.buffer.len() {
            return false;
        }

        self.buffer[self.len] = 0;
        self.len += 1;
        self.count += 1;
        true
    }
}

impl Default for ArgvBlob {
    fn default() -> Self {
        Self::new()
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
/// Waits for a process to exit.
///
/// ## Returns
/// Its exit status, which the kernel masks to a byte.
pub fn wait_for_process(pid: usize) -> Result<usize> {
    decode(unsafe { syscall1(SYS_WAIT_FOR_PROCESS, pid) })
}

/// Fills a buffer with the entries of the working directory.
///
/// ## Returns
/// The number of entries written, where `0` is an empty directory.
pub fn read_dir(entries: &mut [DirEntry]) -> Result<usize> {
    decode(unsafe { syscall2(SYS_READ_DIR, entries.as_mut_ptr() as usize, entries.len()) })
}

pub fn cd(path: &[u8]) -> Result<()> {
    decode(unsafe { syscall2(SYS_CD, path.as_ptr() as usize, path.len()) }).map(|_| ())
}

/// Opens a file.
///
/// ## Arguments
///
/// - `path` the path to open
/// - `flags` an access mode, one of [`O_RDONLY`], [`O_WRONLY`] or [`O_RDWR`],
/// optionally combined with [`O_CREAT`], [`O_EXCL`], [`O_TRUNC`] and
/// [`O_APPEND`]
pub fn open(path: &[u8], flags: usize) -> Result<usize> {
    decode(unsafe { syscall3(SYS_OPEN, path.as_ptr() as usize, path.len(), flags) })
}

/// Creates a new file, failing when it already exists.
///
/// The same thing as `open` with `O_CREAT | O_EXCL`, kept as its own name
/// because that combination reads as noise at a call site.
pub fn create(path: &[u8]) -> Result<usize> {
    open(path, O_RDWR | O_CREAT | O_EXCL)
}

/// Opens a file, creating it when it does not exist yet.
pub fn open_or_create(path: &[u8], flags: usize) -> Result<usize> {
    open(path, flags | O_CREAT)
}

pub fn mkdir(path: &[u8]) -> Result<()> {
    decode(unsafe { syscall2(SYS_MKDIR, path.as_ptr() as usize, path.len()) }).map(|_| ())
}

pub fn unlink(path: &[u8]) -> Result<()> {
    decode(unsafe { syscall2(SYS_UNLINK, path.as_ptr() as usize, path.len()) }).map(|_| ())
}

pub fn rmdir(path: &[u8]) -> Result<()> {
    decode(unsafe { syscall2(SYS_RMDIR, path.as_ptr() as usize, path.len()) }).map(|_| ())
}

pub fn close(fd: usize) -> Result<()> {
    decode(unsafe { syscall1(SYS_CLOSE, fd) }).map(|_| ())
}

/// Open for reading only.
///
/// The values are newlib's, so the libc porting layer can pass its own
/// `O_*` straight through without remapping bits. The access modes are a two
/// bit value rather than independent flags, hence [`O_ACCMODE`].
pub const O_RDONLY: usize = 0x0000;

/// Open for writing only.
pub const O_WRONLY: usize = 0x0001;

/// Open for reading and writing.
pub const O_RDWR: usize = 0x0002;

/// Masks the access mode out of the flags.
pub const O_ACCMODE: usize = 0x0003;

/// Every write goes to the end of the file.
pub const O_APPEND: usize = 0x0008;

/// Create the file when it does not exist.
pub const O_CREAT: usize = 0x0200;

/// Truncate the file to zero length on open.
pub const O_TRUNC: usize = 0x0400;

/// With [`O_CREAT`], fail when the file already exists.
pub const O_EXCL: usize = 0x0800;

/// `st_mode` mask that selects the file type bits.
pub const S_IFMT: u32 = 0o170_000;

/// `st_mode` type bits: a regular file.
pub const S_IFREG: u32 = 0o100_000;

/// `st_mode` type bits: a directory.
pub const S_IFDIR: u32 = 0o040_000;

/// `st_mode` type bits: a character device, which the standard streams are.
pub const S_IFCHR: u32 = 0o020_000;

/// Metadata about a file or directory.
///
/// This mirrors the kernel's layout exactly. It is deliberately not a C
/// `struct stat`; the libc porting layer copies these fields into whatever
/// its own header declares, so the two can be changed independently.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Stat {
    /// Identifies the file within its filesystem. FAT has no inodes, so this
    /// is the file's first cluster, and empty files report zero.
    pub inode: u64,

    /// The file type, one of [`S_IFREG`], [`S_IFDIR`] or [`S_IFCHR`].
    pub mode: u32,

    /// How many names refer to this file, always 1 on FAT.
    pub links: u32,

    /// The size in bytes. Directories report zero.
    pub size: u64,

    /// The filesystem cluster size, the unit reads are most efficient in.
    pub block_size: u32,

    /// How many blocks the file occupies, rounded up to whole clusters.
    pub blocks: u32,

    /// Last access time in seconds since the Unix epoch. FAT records only a
    /// date, so the time of day is always midnight.
    pub accessed_time: i64,

    /// Last modification time in seconds since the Unix epoch.
    pub modified_time: i64,

    /// Creation time in seconds since the Unix epoch.
    pub created_time: i64,
}

impl Stat {
    pub const fn zero() -> Self {
        Self {
            inode: 0,
            mode: 0,
            links: 0,
            size: 0,
            block_size: 0,
            blocks: 0,
            accessed_time: 0,
            modified_time: 0,
            created_time: 0,
        }
    }

    pub fn is_directory(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        self.mode & S_IFMT == S_IFREG
    }

    /// Whether this is a character device, which is what the standard streams
    /// report and what `isatty` is really asking about.
    pub fn is_char_device(&self) -> bool {
        self.mode & S_IFMT == S_IFCHR
    }
}

/// Describes an open file descriptor.
///
/// ## Arguments
///
/// - `fd` the descriptor to describe
/// - `stat` filled in on success
pub fn fstat(fd: usize, stat: &mut Stat) -> Result<()> {
    decode(unsafe { syscall2(SYS_FSTAT, fd, stat as *mut Stat as usize) }).map(|_| ())
}

/// Describes a file or directory by path.
///
/// ## Arguments
///
/// - `path` the path to describe
/// - `stat` filled in on success
pub fn stat(path: &[u8], stat: &mut Stat) -> Result<()> {
    decode(unsafe {
        syscall3(
            SYS_STAT,
            path.as_ptr() as usize,
            path.len(),
            stat as *mut Stat as usize,
        )
    })
    .map(|_| ())
}

/// Whether a descriptor refers to a terminal.
///
/// stdio uses this to choose line buffering over full buffering, which is the
/// difference between output appearing as it is written and appearing only
/// once a buffer fills.
pub fn isatty(fd: usize) -> bool {
    let mut info = Stat::zero();
    match fstat(fd, &mut info) {
        Ok(()) => info.is_char_device(),
        Err(_) => false,
    }
}

/// Moves the offset of an open file descriptor.
///
/// Seeking past the end of a file is allowed; reads there report end of file
/// until a write extends it.
///
/// ## Arguments
///
/// - `fd` the descriptor to seek
/// - `offset` how far to move, relative to `whence`, negative to move back
/// - `whence` [`SEEK_SET`], [`SEEK_CUR`] or [`SEEK_END`]
///
/// ## Returns
/// The new offset from the start of the file.
pub fn lseek(fd: usize, offset: isize, whence: usize) -> Result<usize> {
    decode(unsafe { syscall3(SYS_LSEEK, fd, offset as usize, whence) })
}

/// The current offset of a descriptor, without moving it.
pub fn tell(fd: usize) -> Result<usize> {
    lseek(fd, 0, SEEK_CUR)
}

/// Moves a descriptor back to the start of its file.
pub fn rewind(fd: usize) -> Result<()> {
    lseek(fd, 0, SEEK_SET).map(|_| ())
}

pub fn truncate(fd: usize, size: usize) -> Result<()> {
    decode(unsafe { syscall2(SYS_TRUNCATE, fd, size) }).map(|_| ())
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
pub fn brk(end_data_segment: usize) -> Result<()> {
    decode(unsafe { syscall1(SYS_BRK, end_data_segment) }).map(|_| ())
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
pub fn sbrk(increment: isize) -> Result<usize> {
    let new_break = decode(unsafe { syscall1(SYS_SBRK, increment as usize) })?;

    // the kernel returns the new break, sbrk is defined to hand back the old
    Ok((new_break as isize - increment) as usize)
}

pub fn clock_gettime(clock_id: usize, timespec: &mut Timespec) -> Result<()> {
    decode(unsafe {
        syscall2(
            SYS_CLOCK_GETTIME,
            clock_id,
            timespec as *mut Timespec as usize,
        )
    })
    .map(|_| ())
}

/// Seconds since the Unix epoch, or 0 when the clock is unavailable.
pub fn time() -> i64 {
    let mut timespec = Timespec::zero();
    if clock_gettime(CLOCK_REALTIME, &mut timespec).is_err() {
        return 0;
    }

    timespec.tv_sec
}

/// Nanoseconds since boot, or 0 when the clock is unavailable.
pub fn monotonic_ns() -> i64 {
    let mut timespec = Timespec::zero();
    if clock_gettime(CLOCK_MONOTONIC, &mut timespec).is_err() {
        return 0;
    }

    timespec.tv_sec * NANOSECONDS_PER_SECOND + timespec.tv_nsec
}

pub fn nanosleep(duration: &Timespec) -> Result<()> {
    decode(unsafe { syscall1(SYS_NANOSLEEP, duration as *const Timespec as usize) }).map(|_| ())
}

pub fn sleep_ms(milliseconds: i64) -> Result<()> {
    let duration = Timespec::from_milliseconds(milliseconds);
    nanosleep(&duration)
}

pub fn sleep(seconds: i64) -> Result<()> {
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
