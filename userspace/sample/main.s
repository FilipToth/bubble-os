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

    ; marker for elf_already_run
    mov rbx, 0xFF

read_user_input:
    ; read syscall
    mov rax, 0x03
    int 0x80

    ; output is now in rax
    ; al is the lowest byte
    ; of rax
    cmp al, 0x41
    jne not_matched

    ; check if elf has been
    ; run already
    cmp [elf_already_run], bl
    je not_matched

    mov [elf_already_run], bl

    ; output is upper-case "A",
    ; call execute syscall
    mov rax, 0x04
    mov rdi, elf
    mov rsi, elf_len

    int 0x80

not_matched:
    ; move output to message
    mov [msg], al

    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, msg
    mov rdx, msg_len

    int 0x80

    jmp read_user_input

section .data
    msg db "Hello, World!", 0xA
    msg_len equ $ - msg

    elf db "SAMPLE2 ELF"
    elf_len equ $ - elf
    elf_already_run db 0x00
