//! Console input: a raw byte ring the timer ISR fills, and the line
//! discipline the canonical read syscall runs over it.
//!
//! Both live in the kernel because the ISR runs in whichever address space
//! happened to be active when the tick landed, so a userspace buffer would be
//! written through the wrong page table, and user-controlled indices would be
//! an out of bounds read in here.
//!
//! The discipline runs at read time rather than at input time. That is what
//! lets echo be a property of which syscall the process called: a canonical
//! read echoes what it consumes, a raw read echoes nothing, and neither needs
//! a mode flag anywhere. The cost is that anything typed while nobody is
//! reading sits unechoed until someone does.
//!
//! See .claude/specs/stdin.md.

use spin::Mutex;

use crate::print;

/// Bytes the raw ring holds. One slot is always left empty so a full ring is
/// distinguishable from an empty one, giving `RING_SIZE - 1` of capacity.
const RING_SIZE: usize = 256;

/// Longest line canonical mode will accumulate, including the newline.
pub const LINE_MAX: usize = 256;

const BELL: u8 = 0x07;
const END_OF_TRANSMISSION: u8 = 0x04;
const BACKSPACE: u8 = 0x08;
const DELETE: u8 = 0x7F;
const CARRIAGE_RETURN: u8 = b'\r';
const NEWLINE: u8 = b'\n';

pub struct Console {
    ring: [u8; RING_SIZE],
    head: usize,
    tail: usize,

    /// The line being edited. Separate from the ring because a process can
    /// block halfway through typing one, so the partial line has to outlive
    /// the syscall that was reading it.
    line: [u8; LINE_MAX],
    line_len: usize,

    /// How much of a completed line has already been handed out. A user
    /// buffer smaller than the line takes it across several reads, which is
    /// what newlib's stdio does.
    line_taken: usize,

    line_ready: bool,
}

static CONSOLE: Mutex<Console> = Mutex::new(Console::new());

impl Console {
    const fn new() -> Self {
        Console {
            ring: [0; RING_SIZE],
            head: 0,
            tail: 0,
            line: [0; LINE_MAX],
            line_len: 0,
            line_taken: 0,
            line_ready: false,
        }
    }

    /// Appends a byte to the ring.
    ///
    /// ## Returns
    /// `false` when the ring is full and the byte was dropped.
    fn push_byte(&mut self, byte: u8) -> bool {
        let next = (self.head + 1) % RING_SIZE;
        if next == self.tail {
            return false;
        }

        self.ring[self.head] = byte;
        self.head = next;

        true
    }

    /// Takes the oldest byte out of the ring.
    fn pop_byte(&mut self) -> Option<u8> {
        if self.tail == self.head {
            return None;
        }

        let byte = self.ring[self.tail];
        self.tail = (self.tail + 1) % RING_SIZE;

        Some(byte)
    }

    /// Appends a typed character to the line being edited.
    ///
    /// Stops one short of the buffer so the newline that ends the line always
    /// has somewhere to go, otherwise a line typed right up to the limit
    /// could never be completed and the process would never wake.
    ///
    /// ## Returns
    /// `false` when the line is full and the byte was dropped.
    fn line_push_text(&mut self, byte: u8) -> bool {
        if self.line_len >= LINE_MAX - 1 {
            return false;
        }

        self.line[self.line_len] = byte;
        self.line_len += 1;

        true
    }

    /// Terminates the line being edited.
    ///
    /// Always fits, because `line_push_text` reserved the last slot for it.
    fn line_push_newline(&mut self) {
        if self.line_len < LINE_MAX {
            self.line[self.line_len] = NEWLINE;
            self.line_len += 1;
        }
    }
}

/// Enqueues one byte of input. Called from the timer ISR.
///
/// A dropped byte rings the bell rather than disappearing silently, so a
/// paste that outruns the ring is visible rather than mysterious.
pub fn push(byte: u8) {
    let dropped = {
        let mut console = CONSOLE.lock();
        !console.push_byte(byte)
    };

    if dropped {
        print!("{}", BELL as char);
    }
}

/// Takes one raw byte, with no echo and no editing.
///
/// ## Returns
/// `None` when no input is waiting, and the caller should block.
pub fn take_byte() -> Option<u8> {
    CONSOLE.lock().pop_byte()
}

/// Runs the line discipline over whatever has arrived since the last call.
///
/// Echoes as it goes, so this is also what makes typing visible.
///
/// ## Returns
/// Whether a complete line is now waiting to be taken.
pub fn poll_line() -> bool {
    let mut console = CONSOLE.lock();

    // a line already waiting is not replaced, the next one does not start
    // until this one has been handed out
    if console.line_ready {
        return true;
    }

    while let Some(byte) = console.pop_byte() {
        match byte {
            CARRIAGE_RETURN | NEWLINE => {
                // terminals send CR for the return key, callers expect the LF
                // that ends a line in a text stream
                console.line_push_newline();
                console.line_ready = true;
                print!("\n");

                return true;
            }

            END_OF_TRANSMISSION => {
                // only on an empty line, otherwise it would silently truncate
                // whatever had been typed before it. A zero length line is
                // what newlib reads as end of file
                if console.line_len == 0 {
                    console.line_ready = true;
                    return true;
                }
            }

            BACKSPACE | DELETE => {
                if console.line_len > 0 {
                    console.line_len -= 1;

                    // back up, paint over the character, back up again
                    print!("\x08 \x08");
                }
            }

            _ => {
                if console.line_push_text(byte) {
                    print!("{}", byte as char);
                } else {
                    print!("{}", BELL as char);
                }
            }
        }
    }

    false
}

/// Copies out of a completed line.
///
/// The line is only cleared once all of it has been taken, so a caller with a
/// small buffer keeps reading the rest of it.
///
/// ## Arguments
///
/// - `out` where the bytes are copied to
///
/// ## Returns
/// The number of bytes copied. Zero on a line that is not ready, and zero on
/// a line that ended in end of file.
pub fn take_line(out: &mut [u8]) -> usize {
    let mut console = CONSOLE.lock();
    if !console.line_ready {
        return 0;
    }

    let available = console.line_len - console.line_taken;
    let count = core::cmp::min(out.len(), available);

    let start = console.line_taken;
    out[..count].copy_from_slice(&console.line[start..start + count]);
    console.line_taken += count;

    if console.line_taken >= console.line_len {
        console.line_len = 0;
        console.line_taken = 0;
        console.line_ready = false;
    }

    count
}
