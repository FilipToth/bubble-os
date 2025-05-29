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

mainloop:
    ; yield syscall
    mov rax, 0x05
    int 0x80

    mov rbx, [counter]
    add rbx, 0x01
    mov [counter], rbx

    cmp rbx, 0x0FF
    jne mainloop

final:
    ; print final message
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, msg
    mov rdx, msg_len
    int 0x80

    ; exit syscall
    mov rax, 0x01
    int 0x80

    jmp $

section .data
    msg db "Hello, from Sample ELF 2!", 0xA
    msg_len equ $ - msg
    counter db 0x00
