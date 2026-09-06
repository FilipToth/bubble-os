; fputest - checks that x87 and SSE register state survives a context switch
;
; Run it from the shell with no arguments:
;
;   fputest
;
; It launches a second copy of itself, and both processes then loop yielding
; to each other while checking that their own registers still hold what they
; put there. Expected output, in either order:
;
;   fputest parent: ok
;   fputest child: ok
;
; and `status` reporting 0.
;
; Two processes is the whole point. Nothing else in the system touches xmm,
; because the kernel and every Rust program are built +soft-float, so a lone
; floating point process keeps its registers across a switch whether or not
; the scheduler saves them. The bug only exists between two of them.
;
; To confirm the test can actually fail, comment out the fpu::restore call in
; schedule() and run it again. It should report a mismatch rather than ok.
;
; Written in assembly rather than C so the register contents are chosen here
; instead of by the compiler, and so the checks can read the FXSAVE image
; directly.

%define SYS_EXIT                1
%define SYS_WRITE               2
%define SYS_EXECUTE             4
%define SYS_YIELD               5
%define SYS_WAIT_FOR_PROCESS    6
%define SYS_GETPID              23

%define STDOUT                  1

; Offsets into the 512 byte FXSAVE image, 64-bit layout.
;
; The x87 registers are stored in physical order rather than stack order, and
; which physical register ST0 is depends on TOP, so the checks below compare
; the whole 128 byte block instead of trying to find ST0 inside it.
%define FX_MXCSR                0x18
%define FX_X87                  0x20
%define FX_X87_BYTES            128
%define FX_XMM                  0xA0
%define FX_XMM_BYTES            256

; Both values mask every SSE exception and differ only in the rounding
; control bits, so neither can raise anything, and a process seeing the
; other one's value can only have got it from a missing restore.
%define MXCSR_PARENT            0x1F80  ; round to nearest
%define MXCSR_CHILD             0x3F80  ; round toward -inf

; Enough switches to be convincing without being slow. Both processes run the
; same count so they stay overlapped for most of it; whichever finishes first
; leaves the other checking alone, which proves nothing but costs nothing.
%define ITERATIONS              200

; Result codes, also the index into result_table.
%define RESULT_OK               0
%define RESULT_MXCSR            1
%define RESULT_X87              2
%define RESULT_XMM              3

; Registers held for the whole program:
;   r15  argc, which is also the role: 2 or more means we are the child
;   r14  our pid, the seed for the register pattern
;   r13  the MXCSR value this role installs
;   r12  the child pid, parent only
;   r9   the exit status being accumulated

section .text
    global _start

_start:
    ; the kernel hands us a System V frame with rsp pointing at argc. The
    ; parent passes one extra argument, so argc alone tells us which role we
    ; are and no string comparison is needed
    mov r15, [rsp]

    ; the pattern is seeded from our pid so the two processes cannot
    ; accidentally agree on what they expect to see, which would let a swap
    ; between them pass unnoticed
    mov rax, SYS_GETPID
    int 0x80
    mov r14, rax

    xor r12, r12

    cmp r15, 2
    jge .child

    mov r13, MXCSR_PARENT
    call spawn_child
    mov r12, rax
    test r12, r12
    jz .spawn_failed
    jmp .run

.child:
    mov r13, MXCSR_CHILD

.run:
    call build_pattern
    call load_registers
    call check_loop

    mov r9, rax
    call report

    ; the parent collects the child so the shell sees one combined result,
    ; and so the prompt cannot come back while the child is still running
    cmp r15, 2
    jge .exit
    test r12, r12
    jz .exit

    mov rax, SYS_WAIT_FOR_PROCESS
    mov rdi, r12
    int 0x80

    test rax, rax
    jz .exit
    mov r9, 1

.exit:
    ; any failure collapses to 1, well clear of the 128 + vector range the
    ; kernel uses for a process it killed
    test r9, r9
    jz .status_ready
    mov r9, 1

.status_ready:
    mov rdi, r9
    mov rax, SYS_EXIT
    int 0x80
    jmp $

.spawn_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_spawn_failed]
    mov rdx, msg_spawn_failed_len
    int 0x80

    mov rdi, 1
    mov rax, SYS_EXIT
    int 0x80
    jmp $

; ---------------------------------------------------------------------------
; spawn_child - launches a second copy of this program
;
; The child gets an extra argv entry so it takes the other branch in _start
; and does not spawn a third. Nothing waits for it here: both processes have
; to be runnable at the same time, which is the only situation in which an
; unsaved register file can be observed.
;
; returns the child pid, or 0 if the launch failed
; ---------------------------------------------------------------------------
spawn_child:
    mov rax, SYS_EXECUTE
    lea rdi, [rel child_path]
    mov rsi, child_path_len
    lea rdx, [rel child_argv]
    mov r10, child_argv_len
    mov r8, 2
    int 0x80

    ; pids start at 1, errors come back as small negatives
    cmp rax, 0
    jg .done
    xor rax, rax

.done:
    ret

; ---------------------------------------------------------------------------
; build_pattern - fills `expected` with this process' sixteen xmm values
;
; Register i gets our pid in the high half of the low qword and i in the low
; half, then the bitwise complement of that in the high qword. A value that
; turns up in the wrong register, or in the wrong process, or that was merely
; zeroed, is distinguishable from a correct one.
; ---------------------------------------------------------------------------
build_pattern:
    lea rdi, [rel expected]
    xor rcx, rcx

.next:
    mov rax, r14
    shl rax, 32
    or rax, rcx
    mov [rdi], rax
    not rax
    mov [rdi + 8], rax

    add rdi, 16
    inc rcx
    cmp rcx, 16
    jb .next

    ret

; ---------------------------------------------------------------------------
; load_registers - puts this process' state into the register file
;
; Covers all three things FXSAVE carries: the x87 stack, MXCSR, and xmm0-15.
; A scheme that switched only xmm would still pass the xmm check, so the
; other two are what catch a partial implementation.
; ---------------------------------------------------------------------------
load_registers:
    ; x87: push our pid as an 80-bit value
    fninit
    mov [rel pid_scratch], r14
    fild qword [rel pid_scratch]

    ; sse: xmm0-15 from the pattern, movdqa because `expected` is aligned
    lea rsi, [rel expected]
    movdqa xmm0, [rsi]
    movdqa xmm1, [rsi + 16]
    movdqa xmm2, [rsi + 32]
    movdqa xmm3, [rsi + 48]
    movdqa xmm4, [rsi + 64]
    movdqa xmm5, [rsi + 80]
    movdqa xmm6, [rsi + 96]
    movdqa xmm7, [rsi + 112]
    movdqa xmm8, [rsi + 128]
    movdqa xmm9, [rsi + 144]
    movdqa xmm10, [rsi + 160]
    movdqa xmm11, [rsi + 176]
    movdqa xmm12, [rsi + 192]
    movdqa xmm13, [rsi + 208]
    movdqa xmm14, [rsi + 224]
    movdqa xmm15, [rsi + 240]

    mov [rel mxcsr_scratch], r13d
    ldmxcsr [rel mxcsr_scratch]

    ; snapshot the x87 half now that it is loaded, so the loop has something
    ; to compare against without this program needing to know how an 80-bit
    ; long double is encoded or where TOP put it
    lea rdi, [rel fx_area]
    fxsave64 [rdi]

    lea rsi, [rdi + FX_X87]
    lea rdi, [rel expected_x87]
    mov rcx, FX_X87_BYTES

.copy:
    mov al, [rsi]
    mov [rdi], al
    inc rsi
    inc rdi
    dec rcx
    jnz .copy

    ret

; ---------------------------------------------------------------------------
; check_loop - yields and re-checks, ITERATIONS times
;
; returns one of the RESULT_ codes, stopping at the first mismatch
; ---------------------------------------------------------------------------
check_loop:
    mov rbx, ITERATIONS

.iteration:
    ; handing the cpu over is what makes this a test rather than a read back
    mov rax, SYS_YIELD
    int 0x80

    call check_state
    test rax, rax
    jnz .done

    dec rbx
    jnz .iteration

    mov rax, RESULT_OK

.done:
    ret

; ---------------------------------------------------------------------------
; check_state - compares the live register file against what we loaded
;
; FXSAVE is not privileged, so ring 3 can take its own snapshot and read the
; image directly. Only the three windows this program set are compared: the
; rest of the image holds things like FIP and the tag word that move on their
; own and would produce spurious failures.
;
; returns one of the RESULT_ codes
; ---------------------------------------------------------------------------
check_state:
    lea rdi, [rel fx_area]
    fxsave64 [rdi]

    mov eax, [rdi + FX_MXCSR]
    cmp eax, r13d
    jne .bad_mxcsr

    lea rsi, [rel expected_x87]
    lea rdx, [rdi + FX_X87]
    mov rcx, FX_X87_BYTES
    call compare
    test rax, rax
    jnz .bad_x87

    lea rsi, [rel expected]
    lea rdx, [rdi + FX_XMM]
    mov rcx, FX_XMM_BYTES
    call compare
    test rax, rax
    jnz .bad_xmm

    mov rax, RESULT_OK
    ret

.bad_mxcsr:
    mov rax, RESULT_MXCSR
    ret

.bad_x87:
    mov rax, RESULT_X87
    ret

.bad_xmm:
    mov rax, RESULT_XMM
    ret

; ---------------------------------------------------------------------------
; compare - byte compares two buffers
;
;   rsi  first buffer
;   rdx  second buffer
;   rcx  length
;
; returns 0 when they match, 1 otherwise. Leaves rdi alone, the caller is
; holding the FXSAVE image address in it.
; ---------------------------------------------------------------------------
compare:
    xor rax, rax

.next:
    test rcx, rcx
    jz .done

    mov r8b, [rsi]
    cmp r8b, [rdx]
    jne .differ

    inc rsi
    inc rdx
    dec rcx
    jmp .next

.differ:
    mov rax, 1

.done:
    ret

; ---------------------------------------------------------------------------
; report - prints the outcome as a single write
;
;   rax  the RESULT_ code
;
; One write rather than a role prefix followed by a result, because the other
; process is still running and two writes would interleave mid-line.
; ---------------------------------------------------------------------------
report:
    mov rcx, rax

    ; the child's four entries follow the parent's four
    cmp r15, 2
    jl .role_ready
    add rcx, 4

.role_ready:
    shl rcx, 4                  ; 16 bytes per entry
    lea r11, [rel result_table]
    add r11, rcx

    mov rsi, [r11]
    mov rdx, [r11 + 8]
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    int 0x80

    ret

section .rodata

child_path:         db "/bin/fputest.elf"
child_path_len      equ $ - child_path

; argv[0] is the name as typed, argv[1] is what makes the child a child. The
; kernel wants each entry NUL terminated and the count passed separately
child_argv:         db "fputest", 0, "child", 0
child_argv_len      equ $ - child_argv

msg_spawn_failed:   db "fputest: could not launch the child, nothing to test against", 0xA
msg_spawn_failed_len equ $ - msg_spawn_failed

msg_p_ok:           db "fputest parent: ok", 0xA
msg_p_ok_len        equ $ - msg_p_ok
msg_p_mxcsr:        db "fputest parent: MXCSR changed under us", 0xA
msg_p_mxcsr_len     equ $ - msg_p_mxcsr
msg_p_x87:          db "fputest parent: x87 registers changed under us", 0xA
msg_p_x87_len       equ $ - msg_p_x87
msg_p_xmm:          db "fputest parent: xmm registers changed under us", 0xA
msg_p_xmm_len       equ $ - msg_p_xmm

msg_c_ok:           db "fputest child: ok", 0xA
msg_c_ok_len        equ $ - msg_c_ok
msg_c_mxcsr:        db "fputest child: MXCSR changed under us", 0xA
msg_c_mxcsr_len     equ $ - msg_c_mxcsr
msg_c_x87:          db "fputest child: x87 registers changed under us", 0xA
msg_c_x87_len       equ $ - msg_c_x87
msg_c_xmm:          db "fputest child: xmm registers changed under us", 0xA
msg_c_xmm_len       equ $ - msg_c_xmm

; indexed by role * 4 + result code, sixteen bytes per entry
    align 16
result_table:
    dq msg_p_ok,    msg_p_ok_len
    dq msg_p_mxcsr, msg_p_mxcsr_len
    dq msg_p_x87,   msg_p_x87_len
    dq msg_p_xmm,   msg_p_xmm_len
    dq msg_c_ok,    msg_c_ok_len
    dq msg_c_mxcsr, msg_c_mxcsr_len
    dq msg_c_x87,   msg_c_x87_len
    dq msg_c_xmm,   msg_c_xmm_len

section .bss align=16

    alignb 16
fx_area:        resb 512        ; scratch for our own FXSAVE snapshots
    alignb 16
expected:       resb FX_XMM_BYTES
expected_x87:   resb FX_X87_BYTES
pid_scratch:    resq 1
mxcsr_scratch:  resd 1
