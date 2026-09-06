# FPU state on context switch

`enable_sse` in `boot.s` turned on SSE for the C userspace, but nothing saves or
restores xmm across a switch, so two processes doing floating point corrupt each
other. Fix it eagerly: FXSAVE the outgoing process, FXRSTOR the incoming one, on
every switch. No lazy `CR0.TS` scheme; `TS` stays clear forever and the `MP` bit
set in `enable_sse` becomes vestigial.

Eager costs ~200-400 cycles per switch, which at `PIT_HZ = 100` is under 0.01% of
a core, and far less than the `mov cr3` and the `Process::clone` the switch path
already does. Lazy would only win for processes that never touch xmm, and every C
program touches it, because gcc vectorises struct copies and inlined `memcpy`.

### Where the state lives

512 bytes per process, allocated at `Process::from` and freed in `exit_current`,
the same ownership pattern `stack` and `ring3_page_table` already use. The field
is the **address**, not the buffer: `Process` is `#[derive(Clone)]` and is cloned
on every call to `next_process`, so an inline `[u8; 512]` would be memcpy'd per
switch to serve a buffer only touched at save and restore.

`FXSAVE` `#GP`s on an unaligned operand, so allocate with
`Layout::from_size_align(512, 16)`.

### Initial state

A zeroed area must never be restored. `FXRSTOR` of zeros sets `MXCSR = 0`, which
unmasks every SSE exception, and it `#GP`s outright on reserved MXCSR bits set.

Capture a template **once at boot**, right after `enable_sse` and before any
process exists: `FNINIT`, `LDMXCSR 0x1F80`, then `FXSAVE` into a static. Every
new process gets a copy of it. This cannot be done per process later on, because
`FNINIT` would clobber the x87 state of whichever process is running.

### Where the save and restore go

Save alongside the existing context save in `next_process`, into
`processes[current_index]`, under the same condition and in the same place. The
frame reaching there is already guaranteed to describe the current process
running in ring 3.

Restore in `schedule`, after the page table switch and immediately before
`jump`. The area is kernel memory and the ring 3 table is cloned from the kernel
table, so it is mapped either side of the switch.

### Also needed

Register `#XM` (vector 19) in the IDT, routed into `handle_fault` like the other
userspace-triggerable faults. `CR4.OSXMMEXCPT` is already set, so today an
unmasked SSE exception hits an absent vector and triple faults. `#MF` (vector 16)
is worth adding in the same pass.

### Invariants this depends on

The kernel must never touch xmm between a save and the matching restore. That
holds because `x86_64-bubble-os.json` sets `-mmx,-sse,+soft-float`; it is now
load-bearing rather than incidental and should say so in a comment.

`FXSAVE` covers xmm0-15 only. If anything in userspace is ever built with AVX the
upper halves are silently lost, so `CFLAGS_FOR_TARGET` must stay on the SSE2
baseline. `XSAVE` is the answer if that ever changes, and is out of scope here.
