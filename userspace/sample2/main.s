section .bss
    resb 4096

align 0x08

section .text
    global _start

_start:
    mov rax, stack_top
    mov rsp, rax

    mov rax, 0x04
    int 0x80

    mov r8, 0xADED

    jmp $

section .bss
stack_bottom:
    resb 4096
stack_top:
