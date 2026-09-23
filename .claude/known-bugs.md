# Known bugs and deferred work

Things noticed in passing that were not worth derailing whatever was being
done at the time. Not a roadmap, a place to stop forgetting. Delete an entry
when it is fixed.

## Memory

**The kernel heap ignores alignment.** `allocate_internal` (`src/mem/heap.rs:69`)
takes the alignment as `_align` and drops it, always returning
`block.address + size_of::<Block>()`, so nothing can rely on more than 8 bytes.
Any `#[repr(align(N))]` type with N > 8 behind a `Box` or `Vec` is silently
misaligned, which is UB. Fixing it means `dealloc_internal` can no longer
assume the header sits immediately before the payload. Worked around by hand in
`arch/x86_64/fpu.rs` after it caused a `#GP` in `fxrstor64`.

**Page table allocation failure panics.** `next_table_create`
(`src/mem/paging/page_table.rs:545`) returns a `PageTable` rather than an
`Option` and unwraps inside. Running out of slots takes the kernel down instead
of failing the syscall that asked.

**`elf::load` reads whole binaries into the kernel heap.** `File::read` returns
a `Region` holding the entire file, against a 16 MiB heap. Fine for the ~460 KB
programs today, a hard ceiling well before anything CPython sized. Streaming
segments out of the file would remove both the ceiling and the copy.

## Scheduling

**`exit_current` skips a process after index 0 exits.** It sets
`CURRENT_INDEX = current_index - 1` floored at zero, so whichever process
shifts down into slot 0 misses a turn. Fairness only, nothing breaks.

**`park()` on the error paths still holds the memory controller lock.** The
three failure exits in `schedule` log and park, but the guard is still alive,
so the next tick spins on it. Those paths already mean the kernel is broken;
they should say so rather than hang.

**The syscall restart assumes `int 0x80`.** `read` and `read_char` rewind `rip`
by 2 to re-execute the entry instruction. Correct while `CD 80` is the only way
in, silently wrong the day a `syscall` instruction path is added.

## Interrupts

**`register_interrupt` installs handlers ring 3 can invoke.** Its `is_ring3`
flag maps to the opposite DPL (`src/arch/x86_64/idt.rs`): true gives Ring0,
false gives Ring3. The only caller, the e1000 driver, passes `false` meaning
"not for userspace" and would get a gate any program can trigger with a bare
`int`. Dead while the network stack is commented out, and the first thing to
bite when it is switched back on. Compare `IDT[0x80]`, which sets Ring3
deliberately because `int 0x80` is the syscall entry.

## Syscalls and filesystem

**Filesystem errors collapse to `ENOENT`.** `mkdir` over an existing directory
should be `EEXIST`, `rmdir` on a non-empty one `ENOTEMPTY`, and `cd` onto a
file `ENOTDIR`.

**Fault exit statuses are ambiguous.** A process killed by a fault exits with
`128 + vector`, which a program calling `exit(142)` deliberately cannot be told
apart from.

**Missing POSIX surface.** No `dup`, `dup2`, `pipe`, `getcwd`, or `lstat`.
`_link` and `_fork` stay `ENOSYS`: FAT has no hard links and there is no way to
duplicate a running image, so neither can be implemented rather than merely
being absent.

**`rename` cannot move a directory to a different parent.** Files go anywhere;
a directory can only be renamed where it is. Moving one needs its `..` rewritten
and a walk up the tree to reject a move into its own subtree. Deferred because
that is where the corruption risk sits and nothing needs it yet.

**`getentropy` is undefined in `libc.a`.** Only `arc4random` references it and
nothing pulls that in yet. CPython needs it at interpreter startup.

## Userspace and build

**User stacks are one size for every program.** `USER_STACK_PAGES` is a single
constant, eagerly mapped, so a `sample.elf` that needs a page pays the same
physical memory as a Lua that needs all of it. Raised to 512 KiB for Lua's
parser; CPython would want far more again. Reading a size out of the ELF, or
demand paging, is the way out.

**Eleven of twelve linker scripts lack `.init_array`.** Only `hello/linker.ld`
has the `preinit`/`init`/`fini` array sections that `crt0.S` walks. No program
uses constructors today, so a C program built against any other script would
skip them silently.

**Programs with no `.rodata` emit an empty middle `PT_LOAD` at vaddr 0.** The
loader skips zero-size segments, so this is cosmetic until something stops
skipping them.

**`build.mk:193`** — the `test` target depends on `run_without_building`, and
no rule defines it.

**C programs need `-lm`.** `floor`, `ceil`, `fmod`, `sqrt`, `pow`, the trig
functions and `log` all live in `libm.a`, not the `libc.a` copy in
`build/libc`. Only `ldexp`, `frexp` and `modf` are in libc.

## Console

**Type-ahead is not echoed until something reads it.** The line discipline runs
in the read syscall rather than at input time, which is what makes echo follow
from which syscall a program calls instead of needing a mode flag. Anything
typed while no one is reading appears all at once when a read happens.

**A terminal sending CRLF leaves a stray LF.** The `\r` commits the line and
the `\n` stays in the ring, so the next read sees an empty line and the shell
prints an extra prompt. Not observed with QEMU's raw stdio, which sends bare
`\r`.
