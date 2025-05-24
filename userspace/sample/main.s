section .bss
    resb 4096

align 0x08

section .text
    global _start

_start:
    mov rax, stack_top
    mov rsp, rax

read_user_input:
    ; read syscall
    mov rax, 0x03
    int 0x80

    ; output is now in rax
    ; move output to message
    ; al is the lowest byte
    ; of rax
    mov [msg], al

    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, msg
    mov r11, msg_len

    int 0x80

    mov r8, 0xDEAD

    jmp read_user_input

section data
    msg db "Hello, World!", 0xA
    msg_len equ $ - msg

section .bss
stack_bottom:
    resb 4096
stack_top:
