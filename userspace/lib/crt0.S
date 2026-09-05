/* C runtime startup for bubble-os.
 *
 * The kernel drops a process straight at the ELF entry point with a System V
 * argument frame on the stack and nothing else set up. Everything between
 * that and main() happens here.
 *
 * The frame is written by write_args_frame in src/elf/mod.rs and documented
 * in .claude/specs/syscall-apis.md:
 *
 *     rsp -> [argc][argv[0]]..[argv[argc-1]][NULL][envp[0]]..[NULL]..[strings]
 *
 * rsp is already 16-byte aligned and points at argc, which is exactly what a
 * stock libc _start expects, so no fixups are needed to get here.
 *
 * Intel syntax, to match the nasm sources everywhere else in the tree. This
 * is still assembled by x86_64-elf-gcc rather than nasm, since the weak
 * symbols below want gas directives, so the flavour has to be selected in
 * the file rather than on the command line.
 */

	.intel_syntax noprefix

	.section .text
	.global _start
	.type _start, @function
_start:
	/* A null rbp terminates a backtrace at the outermost frame. */
	xor		ebp, ebp

	mov		rdi, [rsp]				/* argc                          */
	lea		rsi, [rsp + 8]			/* argv                          */
	lea		rdx, [rsi + rdi*8 + 8]	/* envp = argv + (argc + 1) * 8  */

	/* getenv reads the environment through this. syscalls.c initialises it
	 * to an empty array, so it is already valid if we never get here. */
	mov		[rip + environ], rdx

	/* Park the three arguments in callee-saved registers so
	 * __libc_init_array cannot clobber them. */
	mov		rbx, rdi
	mov		r12, rsi
	mov		r13, rdx

	/* Already true of the frame the kernel writes. Doing it here makes the
	 * alignment a local guarantee rather than one inherited from the
	 * kernel, so a change over there cannot silently misalign every call
	 * below. */
	and		rsp, -16

	/* Runs .init_array: anything marked __attribute__((constructor)).
	 *
	 * Weak, because whether newlib builds init.c depends on the target's
	 * have_init_fini setting in configure.host, and an undefined symbol
	 * here would fail the link over constructors that no program uses
	 * yet. An undefined weak resolves to 0 in a static non-PIE link.
	 *
	 * movabs, not lea: a rip-relative lea emits an R_X86_64_PC32, and
	 * the distance from .text at 0x700040000000 down to the 0 an
	 * undefined weak resolves to does not fit in 32 bits, so the link
	 * fails with a relocation overflow. The 64-bit absolute form holds
	 * both 0 and a real address. */
	movabs	rax, offset __libc_init_array
	test	rax, rax
	jz		.Lskip_init
	call	rax

.Lskip_init:
	mov		rdi, rbx
	mov		rsi, r12
	mov		rdx, r13
	call	main

	/* exit, not _exit: it runs atexit handlers and .fini_array and flushes
	 * stdio before calling _exit. Dropping that would lose the tail of any
	 * buffered output. */
	mov		edi, eax
	call	exit

	/* Unreachable in practice, and deliberately kept anyway.
	 *
	 * exit does not return, and _exit in userspace/lib/syscalls.c already
	 * spins forever if SYS_EXIT ever comes back, so that case never reaches
	 * here. This only catches an exit that returns without getting as far
	 * as _exit, which nothing in newlib does today.
	 *
	 * hlt is privileged, so at CPL 3 it is a #GP and the kernel kills the
	 * process. That is the point: falling off the end of .text instead
	 * would run into whatever follows. */
.Lhalt:
	hlt
	jmp		.Lhalt

	.size _start, . - _start

	.weak __libc_init_array

/* Normally supplied by crti.o/crtn.o, which a -nostdlib link does not pull
 * in. newlib's __libc_init_array calls _init when _HAVE_INIT_FINI is set,
 * so provide no-op weak definitions rather than let that decide whether the
 * link succeeds. */
	.weak _init
	.type _init, @function
_init:
	ret

	.weak _fini
	.type _fini, @function
_fini:
	ret
