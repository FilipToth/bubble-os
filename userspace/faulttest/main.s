; faulttest - deliberately raises CPU exceptions from ring 3
;
; Every test below is a label that triggers one fault. Pick which one runs
; by changing the single jmp in _start, rebuild, and run faulttest.elf from
; the shell:
;
;   test_divide_error           #DE  divide by zero
;   test_invalid_opcode         #UD  ud2
;   test_page_fault_unmapped    #PF  write to an unmapped address
;   test_page_fault_kernel      #PF  read kernel memory
;   test_page_fault_text        #PF  write to our own code
;   test_gp_privileged          #GP  cli
;   test_gp_non_canonical       #GP  write through a non canonical pointer
;   test_gp_int3                #GP  int3
;   test_stack_fault            #SS  push with a non canonical rsp
;   test_none                        control case, no fault at all
;
; The expected result for every faulting test is the same:
;   1. the announce line is printed by the program
;   2. the kernel logs the fault and "killing pid N after <fault> ..."
;   3. the shell comes back with a prompt
;
; Anything else (a hang, a reboot, a double fault) is a kernel bug.
;
; Each test ends with a jmp to end so it cannot fall through into the next
; one. A test reaching that jmp means it did not fault.

section .text
    global _start

; prints a message, %1 is the label of a string in .data that has a
; matching %1_len constant
%macro announce 1
    mov rax, 0x02           ; write
    mov rdi, 0x01           ; stdout
    mov rsi, %1
    mov rdx, %1 %+ _len
    int 0x80
%endmacro

_start:
    ; --- the test selector, change this label to run a different test ---
    ; jmp test_divide_error
    ; jmp test_invalid_opcode
    ; jmp test_page_fault_unmapped
    ; jmp test_page_fault_kernel
    ; jmp test_page_fault_text
    ; jmp test_gp_privileged
    ; jmp test_gp_non_canonical
    ; jmp test_gp_int3
    ; jmp test_stack_fault
    jmp test_none

; ---------------------------------------------------------------------------
; #DE - divide error
;
; Integer division by zero. Needs IDT.divide_error to be registered,
; without a handler this vector triple faults the machine.
; ---------------------------------------------------------------------------
test_divide_error:
    announce msg_divide_error
    xor rdx, rdx
    mov rax, 1
    xor rcx, rcx
    div rcx                 ; rdx:rax / 0
    jmp end

; ---------------------------------------------------------------------------
; #UD - invalid opcode
;
; ud2 is the architecturally guaranteed "always invalid" instruction.
; Also covers what a corrupted or misaligned code page looks like.
; ---------------------------------------------------------------------------
test_invalid_opcode:
    announce msg_invalid_opcode
    ud2
    jmp end

; ---------------------------------------------------------------------------
; #PF - page fault, write to an unmapped address
;
; A canonical user address that no process ever maps. The kernel should
; report a page fault with cr2 = 0x6FFF00000000 and a write access.
; ---------------------------------------------------------------------------
test_page_fault_unmapped:
    announce msg_page_fault_unmapped
    mov rax, 0x00006FFF00000000
    mov qword [rax], 0x1234
    jmp end

; ---------------------------------------------------------------------------
; #PF - page fault, read from kernel memory
;
; The low addresses are mapped for the kernel but without the user bit,
; so this is a protection violation rather than a missing page. This is
; the test that proves ring 3 cannot read kernel memory.
; ---------------------------------------------------------------------------
test_page_fault_kernel:
    announce msg_page_fault_kernel
    mov rax, qword [0x100000]
    jmp end

; ---------------------------------------------------------------------------
; #PF - page fault, write to our own code
;
; Only faults if the ELF loader maps .text without the writable flag.
; If this one prints the announce line and then exits cleanly, the text
; segment is still writable from ring 3.
; ---------------------------------------------------------------------------
test_page_fault_text:
    announce msg_page_fault_text
    mov rax, 0x9090909090909090
    mov qword [rel _start], rax
    jmp end

; ---------------------------------------------------------------------------
; #GP - general protection fault, privileged instruction
;
; cli needs CPL <= IOPL, and userspace runs with CPL 3 and IOPL 0.
; ---------------------------------------------------------------------------
test_gp_privileged:
    announce msg_gp_privileged
    cli
    jmp end

; ---------------------------------------------------------------------------
; #GP - general protection fault, non canonical address
;
; Bits 63:48 of a data address must all match bit 47. This one is the
; usual way a wild pointer shows up as a #GP instead of a #PF.
; ---------------------------------------------------------------------------
test_gp_non_canonical:
    announce msg_gp_non_canonical
    mov rax, 0x8000000000000000
    mov qword [rax], 0x1234
    jmp end

; ---------------------------------------------------------------------------
; #GP - general protection fault, int3 from ring 3
;
; The breakpoint gate has DPL 0, so a software int3 from userspace is a
; permission violation rather than a breakpoint.
; ---------------------------------------------------------------------------
test_gp_int3:
    announce msg_gp_int3
    int3
    jmp end

; ---------------------------------------------------------------------------
; #SS - stack segment fault
;
; A stack access through a non canonical rsp raises #SS, not #PF. Note
; that this clobbers rsp first, so the announce has to happen before it.
; ---------------------------------------------------------------------------
test_stack_fault:
    announce msg_stack_fault
    mov rsp, 0x8000000000000000
    push rax
    jmp end

; ---------------------------------------------------------------------------
; the control case, no fault. It should print its message and return to
; the shell exactly like any other program.
; ---------------------------------------------------------------------------
test_none:
    announce msg_no_test
    jmp end

end:
    xor edi, edi            ; exit status 0
    mov rax, 0x01           ; exit
    int 0x80

    ; the kernel never schedules us again, but do not run off the end
    ; of the section if it ever does
    jmp $

section .data
    msg_divide_error db "faulttest: dividing by zero (#DE)", 0xA
    msg_divide_error_len equ $ - msg_divide_error

    msg_invalid_opcode db "faulttest: executing ud2 (#UD)", 0xA
    msg_invalid_opcode_len equ $ - msg_invalid_opcode

    msg_page_fault_unmapped db "faulttest: writing to an unmapped address (#PF)", 0xA
    msg_page_fault_unmapped_len equ $ - msg_page_fault_unmapped

    msg_page_fault_kernel db "faulttest: reading kernel memory (#PF)", 0xA
    msg_page_fault_kernel_len equ $ - msg_page_fault_kernel

    msg_page_fault_text db "faulttest: writing to our own code (#PF)", 0xA
    msg_page_fault_text_len equ $ - msg_page_fault_text

    msg_gp_privileged db "faulttest: executing cli (#GP)", 0xA
    msg_gp_privileged_len equ $ - msg_gp_privileged

    msg_gp_non_canonical db "faulttest: writing to a non canonical address (#GP)", 0xA
    msg_gp_non_canonical_len equ $ - msg_gp_non_canonical

    msg_gp_int3 db "faulttest: executing int3 (#GP)", 0xA
    msg_gp_int3_len equ $ - msg_gp_int3

    msg_stack_fault db "faulttest: pushing with a non canonical rsp (#SS)", 0xA
    msg_stack_fault_len equ $ - msg_stack_fault

    msg_no_test db "faulttest: control case, no fault expected", 0xA
    msg_no_test_len equ $ - msg_no_test
