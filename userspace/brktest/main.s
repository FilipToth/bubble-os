; brktest - exercises the brk and sbrk syscalls from ring 3
;
; Every case prints one line to stdout:
;
;   [ Case N ] - PASSED - description
;   [ Case N ] - FAILED - description
;
; Run it from the shell with brktest.elf. The exit status is the number of
; failed cases, so `status` reporting 0 after a run means everything passed.
;
; The cases build on each other, they are not independent: case 0 records the
; break the process starts with and every later case is written against it.
;
; There is one more check that cannot report its own result, because passing
; means the process dies. See test_past_break at the bottom.

section .text
    global _start

SYS_EXIT    equ 0x01
SYS_WRITE   equ 0x02
SYS_BRK     equ 0x12            ; 18
SYS_SBRK    equ 0x13            ; 19

STDOUT      equ 0x01
PAGE_SIZE   equ 0x1000
MAX_HEAP    equ 0x4000000       ; 64 MiB, the kernel's per process ceiling

; prints a string, %1 is a label in .rodata with a matching %1_len constant
%macro say 1
    mov rsi, %1
    mov rdx, %1 %+ _len
    call puts
%endmacro

; closes a case, r8b already holds 1 for a pass and 0 for a failure
%macro verdict 1
    mov rsi, %1
    mov rdx, %1 %+ _len
    call report
%endmacro

; Registers held for the whole run:
;   r12 - the break the process started with
;   r14 - number of failed cases
;   r15 - the current case number
_start:
    xor r14, r14
    xor r15, r15

    say msg_banner

; --- Case 0: sbrk(0) reports a break, and it is never zero ------------------
    xor rdi, rdi
    mov rax, SYS_SBRK
    int 0x80
    mov r12, rax                    ; every later case is written against this
    test rax, rax
    setnz r8b
    verdict desc_initial

; --- Case 1: sbrk(0) reads the break without moving it ----------------------
    xor rdi, rdi
    mov rax, SYS_SBRK
    int 0x80
    cmp rax, r12
    sete r8b
    verdict desc_query

; --- Case 2: sbrk of one page answers with the new break --------------------
    mov rdi, PAGE_SIZE
    mov rax, SYS_SBRK
    int 0x80
    lea rcx, [r12 + PAGE_SIZE]
    cmp rax, rcx
    sete r8b
    verdict desc_grow_page

; --- Case 3: the first heap byte is readable and writable -------------------
    mov rax, 0x0123456789ABCDEF
    mov [r12], rax
    mov rcx, [r12]
    cmp rcx, rax
    sete r8b
    verdict desc_write_first

; --- Case 4: so is the last byte below the break ----------------------------
    lea rcx, [r12 + PAGE_SIZE - 1]
    mov byte [rcx], 0x5A
    cmp byte [rcx], 0x5A
    sete r8b
    verdict desc_write_last

; --- Case 5: a growth that stays inside one page still moves the break ------
    mov rdi, 16
    mov rax, SYS_SBRK
    int 0x80
    lea rcx, [r12 + PAGE_SIZE + 16]
    cmp rax, rcx
    sete r8b
    verdict desc_grow_partial

; --- Case 6: growing leaves what was already written alone ------------------
    mov rax, 0x0123456789ABCDEF
    mov rcx, [r12]
    cmp rcx, rax
    sete r8b
    verdict desc_survives_grow

; --- Case 7: brk back to the start releases the whole heap ------------------
    mov rdi, r12
    mov rax, SYS_BRK
    int 0x80
    cmp rax, r12
    sete r8b
    verdict desc_release_all

; --- Case 8: a break one byte below the heap start is refused ---------------
    lea rdi, [r12 - 1]
    mov rax, SYS_BRK
    int 0x80
    test rax, rax
    setz r8b
    verdict desc_below_start

; --- Case 9: so is brk(0) ---------------------------------------------------
    xor rdi, rdi
    mov rax, SYS_BRK
    int 0x80
    test rax, rax
    setz r8b
    verdict desc_brk_zero

; --- Case 10: and so is a break past the kernel's heap ceiling --------------
    mov rcx, MAX_HEAP + 1
    lea rdi, [r12 + rcx]
    mov rax, SYS_BRK
    int 0x80
    test rax, rax
    setz r8b
    verdict desc_over_cap

; --- Case 11: a refused request leaves the break where it was ---------------
    xor rdi, rdi
    mov rax, SYS_SBRK
    int 0x80
    cmp rax, r12
    sete r8b
    verdict desc_refusal_clean

; --- Case 12: the heap can be grown again after being fully released --------
    mov rcx, PAGE_SIZE * 3
    lea rdi, [r12 + rcx]
    mov rax, SYS_BRK
    int 0x80
    lea rcx, [r12 + PAGE_SIZE * 3]
    cmp rax, rcx
    sete r8b
    verdict desc_regrow

; --- Case 13: every page of a multi page heap is usable ---------------------
    mov byte [r12], 0x11
    mov rcx, r12
    add rcx, PAGE_SIZE
    mov byte [rcx], 0x22
    add rcx, PAGE_SIZE
    mov byte [rcx], 0x33

    xor r8, r8
    cmp byte [r12], 0x11
    jne .pages_done
    mov rcx, r12
    add rcx, PAGE_SIZE
    cmp byte [rcx], 0x22
    jne .pages_done
    add rcx, PAGE_SIZE
    cmp byte [rcx], 0x33
    jne .pages_done
    mov r8b, 1
.pages_done:
    verdict desc_all_pages

; --- Case 14: a negative increment walks the break back down ----------------
    mov rdi, -(PAGE_SIZE * 2)
    mov rax, SYS_SBRK
    int 0x80
    lea rcx, [r12 + PAGE_SIZE]
    cmp rax, rcx
    sete r8b
    verdict desc_shrink

; --- Case 15: what survives a shrink is still usable ------------------------
    mov byte [r12], 0x77
    cmp byte [r12], 0x77
    sete r8b
    verdict desc_after_shrink

; --- Case 16: shrinking past the heap start is refused ----------------------
    mov rcx, PAGE_SIZE * 100
    neg rcx
    mov rdi, rcx
    mov rax, SYS_SBRK
    int 0x80
    test rax, rax
    setz r8b
    verdict desc_shrink_too_far

; --- Case 17: and the heap can be handed back one last time -----------------
    mov rdi, r12
    mov rax, SYS_BRK
    int 0x80
    cmp rax, r12
    sete r8b
    verdict desc_final_release

; --- summary ----------------------------------------------------------------
    test r14, r14
    jz .all_passed

    say msg_some_failed
    jmp .done

.all_passed:
    say msg_all_passed

.done:
    ; uncomment to run the check that memory past the break is unmapped,
    ; it ends the process with a page fault instead of an exit status
    ; jmp test_past_break

    mov rdi, r14                    ; exit status is the failure count
    mov rax, SYS_EXIT
    int 0x80

    ; the kernel never schedules us again, but do not run off the end
    ; of the section if it ever does
    jmp $

; ---------------------------------------------------------------------------
; The one check that cannot print its own verdict.
;
; The heap was released back to its start above, so nothing at or past the
; break is mapped any more. Touching it has to raise a page fault:
;
;   1. the kernel logs "killing pid N after page fault ..."
;   2. the shell prints "Killed by fault, exception vector 14"
;   3. status reports 142
;
; Reaching the line after the store means the release left the pages behind,
; which is the failure this check exists to catch.
; ---------------------------------------------------------------------------
test_past_break:
    say msg_past_break

    mov rcx, r12
    add rcx, PAGE_SIZE * 4
    mov byte [rcx], 0x41            ; should never complete

    say msg_no_fault
    mov rdi, 1
    mov rax, SYS_EXIT
    int 0x80

; ---------------------------------------------------------------------------
; helpers
; ---------------------------------------------------------------------------

; writes rdx bytes at rsi to stdout
puts:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    int 0x80
    ret

; prints rax in decimal
print_dec:
    mov rsi, dec_buf_end
    xor rcx, rcx
    mov r9, 10

.digit:
    xor rdx, rdx
    div r9                          ; rax = rax / 10, rdx = rax % 10
    add dl, '0'
    dec rsi
    mov [rsi], dl
    inc rcx
    test rax, rax
    jnz .digit

    mov rdx, rcx
    call puts
    ret

; prints one result line, rsi:rdx is the description and r8b the verdict,
; then counts the failure and moves on to the next case number
report:
    push rsi
    push rdx

    say msg_prefix

    mov rax, r15
    call print_dec

    say msg_middle

    test r8b, r8b
    jz .failed

    say msg_passed
    jmp .description

.failed:
    inc r14
    say msg_failed

.description:
    say msg_dash

    pop rdx
    pop rsi
    call puts

    say msg_newline

    inc r15
    ret

section .rodata

msg_banner:         db "brktest - brk and sbrk", 10
msg_banner_len:     equ $ - msg_banner

msg_prefix:         db "[ Case "
msg_prefix_len:     equ $ - msg_prefix

msg_middle:         db " ] - "
msg_middle_len:     equ $ - msg_middle

msg_passed:         db "PASSED"
msg_passed_len:     equ $ - msg_passed

msg_failed:         db "FAILED"
msg_failed_len:     equ $ - msg_failed

msg_dash:           db " - "
msg_dash_len:       equ $ - msg_dash

msg_newline:        db 10
msg_newline_len:    equ $ - msg_newline

msg_all_passed:     db "All cases passed", 10
msg_all_passed_len: equ $ - msg_all_passed

msg_some_failed:    db "Some cases failed, the exit status is the count", 10
msg_some_failed_len: equ $ - msg_some_failed

msg_past_break:     db "Writing past the break, expecting a page fault", 10
msg_past_break_len: equ $ - msg_past_break

msg_no_fault:       db "No fault, released heap pages are still mapped", 10
msg_no_fault_len:   equ $ - msg_no_fault

desc_initial:       db "sbrk(0) reports a non zero starting break"
desc_initial_len:   equ $ - desc_initial

desc_query:         db "sbrk(0) reads the break without moving it"
desc_query_len:     equ $ - desc_query

desc_grow_page:     db "sbrk(PAGE_SIZE) returns the new break"
desc_grow_page_len: equ $ - desc_grow_page

desc_write_first:   db "the first heap byte round trips"
desc_write_first_len: equ $ - desc_write_first

desc_write_last:    db "the last byte below the break round trips"
desc_write_last_len: equ $ - desc_write_last

desc_grow_partial:  db "a growth inside one page still moves the break"
desc_grow_partial_len: equ $ - desc_grow_partial

desc_survives_grow: db "growing leaves existing heap contents alone"
desc_survives_grow_len: equ $ - desc_survives_grow

desc_release_all:   db "brk(start) releases the whole heap"
desc_release_all_len: equ $ - desc_release_all

desc_below_start:   db "a break below the heap start is refused"
desc_below_start_len: equ $ - desc_below_start

desc_brk_zero:      db "brk(0) is refused"
desc_brk_zero_len:  equ $ - desc_brk_zero

desc_over_cap:      db "a break past the heap ceiling is refused"
desc_over_cap_len:  equ $ - desc_over_cap

desc_refusal_clean: db "a refused request leaves the break unchanged"
desc_refusal_clean_len: equ $ - desc_refusal_clean

desc_regrow:        db "the heap can be grown again after a full release"
desc_regrow_len:    equ $ - desc_regrow

desc_all_pages:     db "every page of a three page heap is usable"
desc_all_pages_len: equ $ - desc_all_pages

desc_shrink:        db "a negative sbrk walks the break back down"
desc_shrink_len:    equ $ - desc_shrink

desc_after_shrink:  db "the heap left after a shrink is still usable"
desc_after_shrink_len: equ $ - desc_after_shrink

desc_shrink_too_far: db "shrinking past the heap start is refused"
desc_shrink_too_far_len: equ $ - desc_shrink_too_far

desc_final_release: db "the heap can be handed back a second time"
desc_final_release_len: equ $ - desc_final_release

section .bss

dec_buf:            resb 32
dec_buf_end:
