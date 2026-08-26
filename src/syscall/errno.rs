//! Syscall error numbers and the value convention the dispatcher uses.

/// Error numbers a syscall can fail with.
///
/// The values are the POSIX ones, which agree between Linux and newlib for
/// everything below 35. [`Errno::NoSys`] and [`Errno::NotEmpty`] sit above
/// that line and newlib numbers them differently, so both have to be checked
/// against the `errno.h` of whichever libc gets vendored. A mismatch there
/// does not fail to build, it just reports the wrong error, so it is worth
/// confirming rather than assuming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Errno {
    /// Operation not permitted
    Perm = 1,

    /// No such file or directory
    NoEnt = 2,

    /// No such process
    Srch = 3,

    /// Input or output error
    Io = 5,

    /// Executable format error
    NoExec = 8,

    /// Bad file descriptor
    BadF = 9,

    /// No child processes
    Child = 10,

    /// Out of memory
    NoMem = 12,

    /// Permission denied
    Access = 13,

    /// Bad address, a user pointer that could not be validated
    Fault = 14,

    /// File exists
    Exist = 17,

    /// Not a directory
    NotDir = 20,

    /// Is a directory
    IsDir = 21,

    /// Invalid argument
    Inval = 22,

    /// Too many open files
    MFile = 24,

    /// No space left on device
    NoSpc = 28,

    /// Result out of range
    Range = 34,

    /// Function not implemented
    NoSys = 38,

    /// Directory not empty
    NotEmpty = 39,
}

/// The largest error number the return convention can carry.
///
/// Returns from `-1` to `-MAX_ERRNO` are failures and everything else is a
/// success value. The window is far away from any address userspace can hold,
/// so a returned pointer or break can never be read as an error.
pub const MAX_ERRNO: usize = 4095;

/// What a syscall handler hands back to the dispatcher.
///
/// The two layers are independent:
///
/// - `None` means the handler descheduled the calling process. `rax` belongs
///   to whichever process runs next and must not be touched.
/// - `Some(Ok(value))` returns `value`, `Some(Err(errno))` returns the errno
///   negated.
pub type SyscallResult = Option<Result<usize, Errno>>;

/// Encodes a syscall outcome into the value the caller finds in `rax`.
///
/// ## Arguments
///
/// - `result` what the handler returned
///
/// ## Returns
/// The raw register value.
pub fn encode(result: Result<usize, Errno>) -> usize {
    match result {
        Ok(value) => value,
        Err(errno) => (errno as usize).wrapping_neg(),
    }
}
