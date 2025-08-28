default rel
bits 64

section .bss
    align 16

    stack_bottom:
        resb 4096
    stack_top:

section .text

global _start
_start:
    lea  rsp, [stack_top]

mainloop:
    ; yield syscall
    mov  rax, 0x05
    int  0x80

    movzx ebx, byte [counter]
    add   bl, 1
    mov   byte [counter], bl

    cmp  ebx, 0x1FFF
    jne  mainloop

final:
    ; print final message
    mov  rax, 0x02
    mov  rdi, 0x01
    lea  rsi, [msg]
    mov  rdx, msg_len
    int  0x80

    ; exit syscall
    mov  rax, 0x01
    int  0x80

    jmp  $

section .data
    msg db "Hello, from Sample ELF 2!", 0xA
    msg_len equ $ - msg
    counter db 0x00
