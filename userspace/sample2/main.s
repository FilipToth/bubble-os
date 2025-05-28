section .bss
    stack_bottom:
        resb 4096
    stack_top:

align 0x08

section .text
    global _start

_start:
    mov rax, stack_top
    mov rsp, rax

    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, msg
    mov rdx, msg_len

    int 0x80

    mov r8, 0xADED

    jmp $

section .data
    msg db "Hello from Sample ELF 2!", 0xA
    msg_len equ $ - msg
