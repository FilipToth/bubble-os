# Byte stream stdin

`read` on fd 0 currently ignores the buffer and length it is given, blocks for
one keypress, and returns the character code in `rax`. `ulib::read_stdin_char`
knows that, but newlib's `_read` passes the normal three arguments, so `fgets`
and friends get an untouched buffer and a byte count that is really a
character. Nothing in a C program can read stdin today.

Echo and backspace are also missing: the shell does them by hand in
`read_command`, so any other program types blind.

### Two calls, two modes

Mode is decided by which syscall you make, so there is no termios and no mode
state anywhere.

- **`read` (3)** becomes POSIX for every fd. On fd 0 it blocks until a line is
  complete, echoes as you type, handles backspace, and returns a byte count.
  The trailing newline is included, which is what `fgets` needs. This is what
  newlib, Lua, and the shell use.
- **`read_char` (24)**, new. Returns one byte in `rax`, no echo, no editing.
  This is the current mechanism, kept because `edit` needs raw keystrokes and
  cannot use line mode.

Splitting them this way means `userspace/lib/syscalls.c` needs no change at
all: `_read` already makes the three argument call for every fd, and no fd 0
special case has to leak into the libc port.

### The console

One global structure in the kernel. It cannot live in userspace: the ISR runs
in whatever address space happened to be active, so a user buffer would be
written through the wrong page table, and user-controlled indices would be an
out of bounds read in the kernel.

```rust
struct Console {
    ring: [u8; 256],    // raw bytes, ISR writes, both readers drain
    head: usize,        // circular, drop the newest byte when full
    tail: usize,

    line: [u8; 256],    // the line being edited, canonical mode only
    line_len: usize,
    line_taken: usize,  // how much of a ready line has been handed out
    line_ready: bool,   // newline seen
}
```

The line buffer is separate from the ring because a process can block halfway
through typing a line, and the partial line has to outlive the syscall. It is
global rather than per process for the same reason the ring is: there is one
keyboard.

### Where the discipline runs

In the read syscall, not the ISR. The ISR only enqueues raw bytes.

That is what lets echo be a property of the call rather than a mode flag: a
canonical read echoes what it consumes, a raw read echoes nothing. The cost is
that anything typed while nobody is reading sits unechoed until someone does.

A canonical read drains the ring, and for each byte: a newline commits the
line and sets `line_ready`; a backspace or DEL drops the last byte and echoes
`\b \b`; anything else appends and echoes. Once `line_ready` is set it copies
`min(len, line_len - line_taken)` bytes out, advances `line_taken`, and resets
the line when it is drained. A short user buffer therefore takes a line across
several calls, which is exactly what newlib's stdio does.

Ctrl-D (0x04) on an empty line commits a zero length line, so `read` returns
0 and newlib sees EOF. Without it a REPL can never be exited.

### Blocking

Once `read` returns a count, the current trick of having the waker write the
answer into `rax` no longer works, because the process has to re-run the drain
after it wakes. Rewind `context.rip` by 2 before blocking, the length of
`int 0x80`, so the syscall simply re-executes when the process is scheduled
again. Stateless, and it works for any blocking syscall later.

`process_input` must stop writing into `rax` and stop broadcasting to every
blocking process; it just pushes to the ring and clears `blocking`.

### Overflow

Three places, all of which drop rather than block, and all of which ring the
bell (`\x07`) so the loss is visible.

1. The UART FIFO, which overflows **today**: `timer_isr` reads a single byte
   per tick, so input is capped at 100 bytes/sec while the 16550 holds 16.
   Drain in a `while serial_received()` loop instead.
2. The ring, when nothing is reading. Drop the newest byte, never the oldest:
   losing the start of a line is worse than losing the end.
3. The line buffer, at 256 characters with no newline. Drop and do not echo,
   which is what the shell already does.

### Userspace

- `ulib::read_stdin_char` moves to syscall 24. `edit` is otherwise untouched.
- `ulib::read` is already the right shape and now works on fd 0.
- The shell's `read_command` collapses to one `ulib::read` call; its echo and
  backspace loop is deleted.

### Out of scope

No termios, no raw/cooked switching at runtime, no job control, no signals, so
no Ctrl-C. Word erase and line kill are not worth it until something asks.
