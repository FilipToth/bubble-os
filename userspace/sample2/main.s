default rel
bits 64

; section .bss
    ; align 16

    ; stack_bottom:
    ;     resb 4096
    ; stack_top:

section .text

global _start
_start:
    ; lea  rsp, [stack_top]

    ; print message
    mov  rax, 0x02
    mov  rdi, 0x01
    lea  rsi, [msg]
    mov  rdx, msg_len
    int  0x80

    ; execute sample.elf
    mov rax, 0x04
    mov rdi, elf
    mov rsi, elf_len
    int 0x80

    jmp  $

section .data
    msg db "Hello, from Sample 2", 0xA
    msg_len equ $ - msg

    elf db "sample.elf"
    elf_len equ $ - elf
    counter db 0x00
