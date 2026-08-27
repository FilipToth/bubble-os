# Syscall ABI

The single source of truth for the boundary between ring 3 and the kernel.
Three places have to agree with this document and with each other:

- the kernel handlers in `src/syscall/`
- `userspace/ulib/lib.rs`, the Rust userspace library
- the newlib porting layer, once it exists

A disagreement here does not fail to build. It shows up as a file that empties
itself, an error reported as the wrong error, or a struct field read from the
wrong offset. Anything added below has to be added to every side at once.

## Calling convention

`int 0x80`, syscall number in `rax`, up to five arguments:

| Argument | Register |
| -------- | -------- |
| 0        | `rdi`    |
| 1        | `rsi`    |
| 2        | `rdx`    |
| 3        | `r10`    |
| 4        | `r8`     |

`r10` rather than `rcx` because `rcx` is clobbered by `syscall`/`sysret`; the
choice matches Linux so a libc written against that shape needs no changes.
The return value comes back in `rax`.

### Return values

A syscall returns its result in `rax`. Failures are the errno **negated**:

- `rax` in `-1 ..= -4095` is a failure, the errno is `-rax`
- anything else is a success value

The window is chosen so no legitimate return can land in it. User pointers
live around `0x0000_7000_0000_0000`, program breaks and buffer sizes are
positive and small by comparison, and `-4096` and below stay valid successes so
a syscall returning a high address later still works.

Userspace decodes with:

```c
if (ret < 0 && ret >= -4095) { errno = -ret; return -1; }
```

**Zero is a success.** `read` returning 0 is end of file, `read_dir` returning
0 is an empty directory. Both were ambiguous under the old "0 means failure"
convention and that is the reason this convention exists.

### Descheduling

Some handlers do not return to the caller at all — they deschedule it and jump
into another process. `exit`, `yield`, `read` on a standard stream, `nanosleep`
with a non-zero duration, and `wait_for_process` on a live child all do this.
The kernel represents that as `None` from the handler, distinct from success or
failure, and the dispatcher leaves `rax` alone because it now belongs to
whichever process runs next.

Userspace cannot observe the difference: the value that eventually lands in
`rax` still follows the rules above. Anything that writes a resume value into a
descheduled process must encode it the same way — see `next_process` in
`src/scheduling/mod.rs`, which encodes the child exit status a waiter wakes up
with.

## Error numbers

Defined in `src/syscall/errno.rs`, mirrored in `ulib` as `Errno`.

| Name        | Value | Meaning                                        |
| ----------- | ----- | ---------------------------------------------- |
| `EPERM`     | 1     | Operation not permitted                        |
| `ENOENT`    | 2     | No such file or directory                      |
| `ESRCH`     | 3     | No such process                                |
| `EIO`       | 5     | Input or output error                          |
| `ENOEXEC`   | 8     | Executable format error                        |
| `EBADF`     | 9     | Bad file descriptor                            |
| `ECHILD`    | 10    | No child processes                             |
| `ENOMEM`    | 12    | Out of memory                                  |
| `EACCES`    | 13    | Permission denied                              |
| `EFAULT`    | 14    | Bad address, a user pointer that failed to validate |
| `EEXIST`    | 17    | File exists                                    |
| `ENOTDIR`   | 20    | Not a directory                                |
| `EISDIR`    | 21    | Is a directory                                 |
| `EINVAL`    | 22    | Invalid argument                               |
| `EMFILE`    | 24    | Too many open files                            |
| `ENOSPC`    | 28    | No space left on device                        |
| `ERANGE`    | 34    | Result out of range                            |
| `ENOSYS`    | 38    | Function not implemented                       |
| `ENOTEMPTY` | 39    | Directory not empty                            |

> **Check before porting.** These are the POSIX/Linux values. Everything at or
> below 34 agrees with newlib, but `ENOSYS` and `ENOTEMPTY` do not — newlib
> numbers those differently. Confirm both against the `errno.h` that actually
> gets vendored and remap in the stub if they differ. A mismatch reports the
> wrong error rather than failing to build.

`EFAULT` covers every user pointer that fails validation, and `ESRCH` is also
used when there is no current process — a condition that should be impossible
from ring 3 but is reachable in the handlers.

Failures the filesystem layer cannot yet distinguish are reported as `ENOENT`:
`mkdir` over an existing name, `rmdir` on a non-empty directory, and `open`
without `O_CREAT` on a missing file all look the same from above. Making these
precise needs the `Directory` trait to return a reason instead of an
`Option`, and is worth doing before a C program starts depending on `EEXIST`.

## Shared types

All are `#[repr(C)]` and must be laid out identically on both sides. These are
**our** definitions, not any libc's — the newlib stub translates into whatever
its own headers declare, so a libc that reorders or resizes its structs cannot
silently break the ABI.

### `FileStat` — 56 bytes, 8 byte aligned

| Offset | Type  | Field           | Notes                                              |
| ------ | ----- | --------------- | -------------------------------------------------- |
| 0      | `u64` | `inode`         | FAT has no inodes; this is the first cluster. Empty files report 0 |
| 8      | `u32` | `mode`          | `S_IFREG`, `S_IFDIR` or `S_IFCHR`                  |
| 12     | `u32` | `links`         | Always 1, FAT has no hard links                    |
| 16     | `u64` | `size`          | Bytes. Directories report 0                        |
| 24     | `u32` | `block_size`    | Filesystem cluster size                            |
| 28     | `u32` | `blocks`        | Clusters occupied, rounded up                      |
| 32     | `i64` | `accessed_time` | Unix seconds. FAT stores only a date, so always midnight |
| 40     | `i64` | `modified_time` | Unix seconds                                       |
| 48     | `i64` | `created_time`  | Unix seconds. POSIX `st_ctime` is inode change time, which FAT does not record; the stub reports this in its place |

No padding holes, so the struct copies to userspace as one block.

Mode constants: `S_IFMT` `0o170000`, `S_IFREG` `0o100000`, `S_IFDIR`
`0o040000`, `S_IFCHR` `0o020000`. Only the type bits are ever set — nothing
here has an owner, so permission bits are the stub's business.

The standard streams report `S_IFCHR` with `block_size` 1 and everything else
zero. stdio reads this to choose line buffering over full buffering, so it has
to answer rather than fail.

### `Timespec` — 16 bytes

| Offset | Type  | Field     |
| ------ | ----- | --------- |
| 0      | `i64` | `tv_sec`  |
| 8      | `i64` | `tv_nsec` |

### `DirEntry` — 264 bytes

| Offset | Type       | Field  | Notes                              |
| ------ | ---------- | ------ | ---------------------------------- |
| 0      | `[u8; 256]`| `name` | NUL padded, not NUL terminated when full |
| 256    | `u8`       | `attr` | `0x10` marks a directory           |
| 257    | 3 bytes    | —      | padding                            |
| 260    | `u32`      | `size` | Currently always 0                 |

`size` is written as 0 by the kernel today; use `stat` for a real size.

## Open flags

BSD numbering, which is what newlib's `fcntl.h` uses, so the stub passes its
own `O_*` through without remapping.

| Name       | Value    |
| ---------- | -------- |
| `O_RDONLY` | `0x0000` |
| `O_WRONLY` | `0x0001` |
| `O_RDWR`   | `0x0002` |
| `O_ACCMODE`| `0x0003` |
| `O_APPEND` | `0x0008` |
| `O_CREAT`  | `0x0200` |
| `O_TRUNC`  | `0x0400` |
| `O_EXCL`   | `0x0800` |

The three access modes are a two-bit **value**, not independent flags:
`O_RDONLY | O_WRONLY` is not `O_RDWR`, and the fourth combination (`3`) is
undefined and rejected. Mask with `O_ACCMODE` to read the mode out.

Any bit outside this set is rejected with `EINVAL` rather than ignored, so a
libc using Linux numbering fails loudly instead of treating `O_CREAT` as a
no-op. Note that Linux's `O_CREAT` (`0o100`) is one of the bits we reject.

Mode strings translate as:

| `fopen` mode | Flags                                 |
| ------------ | ------------------------------------- |
| `r`          | `O_RDONLY`                            |
| `r+`         | `O_RDWR`                              |
| `w`          | `O_WRONLY \| O_CREAT \| O_TRUNC`      |
| `w+`         | `O_RDWR \| O_CREAT \| O_TRUNC`        |
| `a`          | `O_WRONLY \| O_CREAT \| O_APPEND`     |
| `a+`         | `O_RDWR \| O_CREAT \| O_APPEND`       |

## Other constants

Seek: `SEEK_SET` 0, `SEEK_CUR` 1, `SEEK_END` 2.

Clocks: `CLOCK_REALTIME` 0, `CLOCK_MONOTONIC` 1.

Standard descriptors: stdin 0, stdout 1, stderr 2. Files start at 3.

## The syscall table

Every entry lists the registers it reads. Unlisted registers are ignored.
"Deschedules" marks the calls that may not return directly to the caller.

### 1 — `exit(status)`

`rdi` status. Never returns; deschedules.

The status is masked to 8 bits, as `WEXITSTATUS` exposes it. That matters for
more than tidiness: `wait_for_process` hands the status back over an ABI where
negative values are errors, and an unmasked `exit(-1)` would come back looking
like `EPERM`. The mask still fits the `128 + vector` statuses the fault handler
uses, which top out at 148.

### 2 — `write(fd, buffer, len) -> count`

`rdi` fd, `rsi` buffer, `rdx` length. Returns bytes written.

fd 1 and 2 go to the console and require valid UTF-8. fd 3 and up write at the
descriptor's offset, extending the file when needed; with `O_APPEND` the offset
moves to the end of the file before every write.

Errors: `EFAULT`, `EBADF` (not open, or not writable), `EINVAL` (invalid UTF-8
to the console).

### 3 — `read(fd, buffer, len) -> count`

`rdi` fd, `rsi` buffer, `rdx` length. Returns bytes read; **0 means end of
file**. Deschedules when fd < 3.

fd 0 blocks until a key is pressed and returns that character. The keyboard
handler writes it straight into the waiting process' `rax`, so it bypasses the
normal return path — it is always a small positive value and so never collides
with the error window.

Errors: `EFAULT`, `EBADF`.

### 4 — `execute(path, path_len, argv, argv_len, argv_count) -> pid`

`rdi` path, `rsi` path length, `rdx` argv blob, `r10` blob bytes, `r8` entry
count. Returns the new pid, which is always ≥ 1.

`argv` is a **NUL-separated blob**, not a joined string: `count` entries laid
end to end, each NUL-terminated, `argv_len` counting every byte including the
terminators. This is what lets an argument contain spaces or quotes. The blob
must hold exactly `count` terminated pieces — trailing bytes without a NUL, or
a count that disagrees with the contents, are rejected rather than parsed as
far as they go.

The caller supplies `argv[0]`, the way `execve` does. A count of 0 gets
`argv[0] = path` so a bare exec is not nameless, and an empty first entry is
backfilled the same way.

Limits: 4096 blob bytes, 64 entries.

Errors: `EFAULT`, `EINVAL` (malformed blob or non-UTF-8), `EPERM` (launching
`shell.elf`), `ERANGE` (over either limit), `ENOENT`, `EIO` (unreadable file),
`ENOEXEC` (ELF load failed), `ENOMEM` (deploy failed).

### 5 — `yield()`

No arguments. Deschedules.

### 6 — `wait_for_process(pid) -> status`

`rdi` pid. Returns the child's exit status, masked to 8 bits. Deschedules when
the child is still running.

A status is already recorded if the child exited between the parent's
`execute` and this call, in which case it returns immediately. Exit records are
capped at 64 and the oldest is dropped on overflow, so a status nobody waits
for is eventually lost.

Errors: `ESRCH` (no such process, and no exit record). Any pid may be waited
on, not only children — `ECHILD` is reserved for if that is ever restricted.

### 7 — `read_dir(buffer, max_entries) -> count`

`rdi` buffer, `rsi` maximum entries. Returns entries written; **0 is an empty
directory**. Reads the current working directory only.

Errors: `EINVAL` (size overflow), `EFAULT`.

### 8 — `cd(path, len)`

`rdi` path, `rsi` length. Returns 0.

Errors: `EFAULT`, `EINVAL` (empty or non-UTF-8), `ENOENT` (also covers a path
that names a regular file, which should become `ENOTDIR`).

### 9 — `open(path, len, flags) -> fd`

`rdi` path, `rsi` length, `rdx` flags. Returns a descriptor ≥ 3.

Handles `O_CREAT`, `O_EXCL`, `O_TRUNC` and `O_APPEND`. There is no `mode`
argument — nothing here has permission bits, so the stub drops libc's.

Errors: `EFAULT`, `EINVAL` (empty path, non-UTF-8, unknown flag bits, undefined
access mode, or `O_TRUNC` without write access), `EEXIST` (`O_CREAT | O_EXCL`
on an existing file), `ENOENT`, `EIO` (truncate failed).

### 10 — `close(fd)`

`rdi` fd. Returns 0. Errors: `EBADF`.

### 11 — `truncate(fd, size)`

`rdi` fd, `rsi` size. Returns 0. Errors: `EBADF`.

### 12 — reserved

Was `create`. Folded into `open` as `O_CREAT`; the number is left unused and
returns `ENOSYS`.

### 13 — `mkdir(path, len)`

`rdi` path, `rsi` length. Returns 0.

Errors: `EFAULT`, `EINVAL`, `ENOENT` (also covers an existing name, which
should become `EEXIST`).

### 14 — `unlink(path, len)`

`rdi` path, `rsi` length. Returns 0. Errors: `EFAULT`, `EINVAL`, `ENOENT`.

### 15 — `rmdir(path, len)`

`rdi` path, `rsi` length. Returns 0.

Errors: `EFAULT`, `EINVAL`, `ENOENT` (also covers a non-empty directory, which
should become `ENOTEMPTY`).

### 16 — `clock_gettime(clock_id, timespec)`

`rdi` clock id, `rsi` a `Timespec` to fill. Returns 0.

Errors: `EINVAL` (unknown clock), `EFAULT`.

### 17 — `nanosleep(timespec)`

`rdi` a `Timespec`. Returns 0. Deschedules unless the duration rounds to zero
ticks.

Errors: `EFAULT`, `EINVAL` (negative fields, `tv_nsec` ≥ 1e9, or a duration
that overflows the tick conversion).

### 18 — `brk(addr) -> new_break`

`rdi` the requested break. Returns the new break.

The heap starts one page past the highest address any ELF segment occupies,
computed at load time. A process that has never called `brk` has a break equal
to that start and no pages behind it. Pages are mapped eagerly — there is no
demand paging, so a fault would kill the process — and the break is recorded to
the byte while memory moves a page at a time.

Errors: `EINVAL` (below the heap start), `ENOMEM` (past the 64 MiB per-process
cap, or the frame allocator ran dry). A failed request leaves the break where
it was.

### 19 — `sbrk(increment) -> new_break`

`rdi` a **signed** increment; negative gives pages back. Returns the new break.

Note this returns the **new** break where POSIX `sbrk` returns the old one —
`ulib` subtracts the increment to convert, and the newlib stub must do the
same. `sbrk(0)` reads the break without moving it, which is how a program finds
where its heap begins.

Errors: as `brk`.

### 20 — `lseek(fd, offset, whence) -> offset`

`rdi` fd, `rsi` a **signed** offset, `rdx` whence. Returns the new offset from
the start of the file.

Seeking past the end is allowed; reads there report end of file until a write
extends the file. Seeking before the start is refused and leaves the offset
untouched.

Errors: `EINVAL` (unknown whence), `EBADF` (not a file, or the result would be
negative).

### 21 — `fstat(fd, stat)`

`rdi` fd, `rsi` a `FileStat` to fill. Returns 0.

Errors: `EBADF`, `EFAULT`.

### 22 — `stat(path, len, stat)`

`rdi` path, `rsi` length, `rdx` a `FileStat` to fill. Returns 0.

Works on both files and directories; files are tried first.

Errors: `EFAULT`, `EINVAL`, `ENOENT`.

### 23 — `getpid() -> pid`

No arguments. Returns the calling process' pid, always ≥ 1.

Errors: `ESRCH`.

## Process entry

A new process starts with a System V style frame at `rsp`:

```
rsp -> [ argc ]
       [ argv[0] ] ... [ argv[argc-1] ]
       [ NULL ]
       [ envp[0] ] ... [ envp[n-1] ]
       [ NULL ]
       ... the strings themselves ...
```

16-byte aligned. `envp` is found at `argv + (argc + 1) * 8`:

```asm
mov rdi, [rsp]                  ; argc
lea rsi, [rsp + 8]              ; argv
lea rdx, [rsi + rdi*8 + 8]      ; envp
```

The user stack is 32 pages (128 KiB) in its own PML4 slot. A crt0 must not
switch stacks before reading the frame — doing so throws away argv and the
environment.

The environment is read-only: entries are inherited from the parent at
`execute` and there is no `setenv`. The first process starts with
`PATH=/bin` and `HOME=/`.

## Process exit

Exit statuses are masked to 8 bits. Statuses of 128 and above mean the kernel
killed the process after a CPU fault, and the value is `128 + vector`:

| Status | Vector | Fault              |
| ------ | ------ | ------------------ |
| 128    | 0      | Divide error       |
| 134    | 6      | Invalid opcode     |
| 140    | 12     | Stack fault        |
| 141    | 13     | General protection |
| 142    | 14     | Page fault         |

This encoding is ambiguous by construction — a process can deliberately
`exit(142)` and be indistinguishable from one that page faulted. Telling them
apart needs a separate flag on the exit record.

## Adding a syscall

1. Add the handler in `src/syscall/`, returning `SyscallResult`. Every failure
   path must return an explicit `Some(Err(..))`; the type makes forgetting
   impossible, which is the point.
2. Register it in `src/syscall/mod.rs` and the dispatcher in
   `src/arch/x86_64/idt.rs`. Never renumber an existing entry.
3. Add the `ulib` wrapper, returning `Result`.
4. Add the newlib stub.
5. Update this document.

Validate every user pointer with `Process::copy_from_user`,
`copy_slice_to_user` or `can_process_pointer` before touching it, and never
size a kernel allocation from an unclamped userspace length.
