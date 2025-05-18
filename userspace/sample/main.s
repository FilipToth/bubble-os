section .bss
    resb 4096

section .text
    global _start

_start:
    mov rax, stack_top
    mov rsp, rax

    mov rax, 0x02
    int 0x80

    mov r8, 0xDEAD

    jmp $

section .bss
stack_bottom:
    resb 4096
stack_top:
