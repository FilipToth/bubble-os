section .bss
    stack_bottom:
        resb 4096
    stack_top:

align 0x08

section .text
    global _start

_start:
    mov ax, cs
    and ax, 0b11
    cmp ax, 3
    je success

    ; not in ring 3
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, failure_msg
    mov rdx, failure_msg_len
    int 0x80

    jmp exit

success:
    ; not in ring 3
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, success_msg
    mov rdx, success_msg_len
    int 0x80

exit:
    ; exit syscall
    ; mov rax, 0x01
    ; int 0x80

    jmp $

section .data
    success_msg db "Hello Ring 3!", 0xA
    success_msg_len equ $ - success_msg

    failure_msg db "Not running in Ring 3 :(", 0xA
    failure_msg_len equ $ - failure_msg
