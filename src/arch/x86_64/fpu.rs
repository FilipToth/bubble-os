//! x87 and SSE register state, saved and restored across a context switch.
//!
//! `enable_sse` in boot.s turns SSE on for the C userspace, which means the
//! register file is now per process state that has to be switched like any
//! other. This is the eager scheme: FXSAVE the outgoing process and FXRSTOR
//! the incoming one on every switch, with `CR0.TS` left clear forever. See
//! .claude/specs/fpu-state.md for why lazy switching was not chosen.
//!
//! The whole thing rests on the kernel never touching xmm itself, so that the
//! registers still hold what ring 3 left in them by the time the scheduler
//! gets around to saving them. That holds because both x86_64-bubble-os.json
//! and the userspace target files set `-mmx,-sse,+soft-float`, and it is now
//! load bearing rather than incidental: dropping `-sse` from the kernel target
//! would corrupt user floating point with no other symptom.

use alloc::alloc::{alloc, dealloc};
use core::alloc::Layout;

/// Bytes an FXSAVE area occupies. Fixed by the instruction.
const FXSAVE_SIZE: usize = 512;

/// FXSAVE and FXRSTOR raise #GP on a destination that is not 16-byte aligned.
const FXSAVE_ALIGN: usize = 16;

/// What is actually asked of the allocator.
///
/// The kernel heap does not honour the alignment in a `Layout`: it computes
/// one in `block_align_size` and then drops it, `allocate_internal` takes it
/// as `_align`, and the address handed back is always `block.address` plus
/// the 32 byte block header. Block addresses drift by whatever sizes came
/// before, so anything asking for more than 8 bytes of alignment gets it only
/// by luck. That silence is why this looked fine until an unrelated change
/// started allocating on every console read.
///
/// So over-allocate by one alignment and find the usable address by hand.
const FXSAVE_ALLOC_SIZE: usize = FXSAVE_SIZE + FXSAVE_ALIGN;

/// Every SSE exception masked, round to nearest, flush-to-zero off.
///
/// FNINIT does not cover this: it only resets the x87 half of the state, and
/// leaves MXCSR alone. A zeroed MXCSR would unmask every SSE exception, so
/// the value has to be written by hand before the template is taken.
const MXCSR_DEFAULT: u32 = 0x1F80;

#[repr(C, align(16))]
struct FxSaveArea([u8; FXSAVE_SIZE]);

/// The state a process starts with, captured once at boot.
///
/// FXRSTOR rejects an area with reserved MXCSR bits set, so a new process
/// cannot simply be handed 512 zeroed bytes.
static mut TEMPLATE: FxSaveArea = FxSaveArea([0; FXSAVE_SIZE]);

/// Builds the initial FPU state every process is given a copy of.
///
/// Must run before the first process exists. FNINIT clobbers the x87 state of
/// whoever is currently running, which is why the template is taken once here
/// rather than built per process at creation time.
pub fn init() {
    let mxcsr: u32 = MXCSR_DEFAULT;

    unsafe {
        core::arch::asm!(
            "fninit",
            "ldmxcsr [{mxcsr}]",
            "fxsave64 [{area}]",
            mxcsr = in(reg) &mxcsr,
            area = in(reg) core::ptr::addr_of_mut!(TEMPLATE),
        );
    }
}

/// The aligned save area inside an allocation from `alloc_state`.
///
/// Derived rather than stored: the offset is fixed by the address the
/// allocator returned, so both halves of a save and restore pair land on the
/// same place without a second field to keep in step.
fn aligned(area: usize) -> usize {
    (area + FXSAVE_ALIGN - 1) & !(FXSAVE_ALIGN - 1)
}

/// Allocates an FPU save area holding the initial state.
///
/// The area is deliberately not a field inside `Process`: that struct is
/// cloned on every pass through the scheduler, and an inline 512-byte array
/// would be copied on every context switch to serve a buffer that is only
/// touched at save and restore.
///
/// ## Returns
/// The address of the area, or `None` when the allocation failed.
pub fn alloc_state() -> Option<usize> {
    let Ok(layout) = Layout::from_size_align(FXSAVE_ALLOC_SIZE, FXSAVE_ALIGN) else {
        return None;
    };

    let area = unsafe { alloc(layout) };
    if area.is_null() {
        return None;
    }

    unsafe {
        core::ptr::copy_nonoverlapping(
            core::ptr::addr_of!(TEMPLATE) as *const u8,
            aligned(area as usize) as *mut u8,
            FXSAVE_SIZE,
        );
    }

    // the raw allocation, not the aligned address inside it, so free_state
    // hands the allocator back exactly what it gave out
    Some(area as usize)
}

/// Releases an FPU save area.
///
/// ## Arguments
///
/// - `area` the address `alloc_state` handed out
pub fn free_state(area: usize) {
    if area == 0 {
        return;
    }

    let Ok(layout) = Layout::from_size_align(FXSAVE_ALLOC_SIZE, FXSAVE_ALIGN) else {
        return;
    };

    unsafe { dealloc(area as *mut u8, layout) };
}

/// Writes the live register state into a save area.
///
/// ## Arguments
///
/// - `area` the address `alloc_state` handed out
pub fn save(area: usize) {
    if area == 0 {
        return;
    }

    let area = aligned(area);
    unsafe {
        core::arch::asm!("fxsave64 [{area}]", area = in(reg) area);
    }
}

/// Loads a save area back into the registers.
///
/// ## Arguments
///
/// - `area` the address `alloc_state` handed out
pub fn restore(area: usize) {
    if area == 0 {
        return;
    }

    let area = aligned(area);
    unsafe {
        core::arch::asm!("fxrstor64 [{area}]", area = in(reg) area);
    }
}
