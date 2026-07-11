default rel
bits 64

section .bss
    stack_bottom:
        resb 4096
    stack_top:

    path_buffer: resb 128
    file_buffer: resb 256

align 0x08

section .text
    global _start

_start:
    lea rax, [rel stack_top]
    mov rsp, rax

    ; print path prompt
    mov rax, 0x02
    mov rdi, 0x01
    lea rsi, [rel prompt]
    mov rdx, prompt_len
    int 0x80

    mov qword [path_len], 0x00

read_path:
    ; read one character from stdin
    mov rax, 0x03
    mov rdi, 0x00
    int 0x80

    cmp rax, 0x0D
    je open_file

    cmp rax, 0x0A
    je open_file

    mov rcx, [path_len]
    cmp rcx, 0x7F
    jae read_path

    lea rdx, [rel path_buffer]
    mov [rdx + rcx], al
    mov [char_buffer], al

    inc rcx
    mov [path_len], rcx

    ; echo typed character
    mov rax, 0x02
    mov rdi, 0x01
    lea rsi, [rel char_buffer]
    mov rdx, 0x01
    int 0x80

    jmp read_path

open_file:
    ; print newline after input
    mov rax, 0x02
    mov rdi, 0x01
    lea rsi, [rel newline]
    mov rdx, newline_len
    int 0x80

    cmp qword [path_len], 0x00
    je open_error

    ; open path
    mov rax, 0x09
    lea rdi, [rel path_buffer]
    mov rsi, [path_len]
    int 0x80

    mov [file_fd], rax
    cmp rax, 0x00
    je open_error

    ; read up to 256 bytes from file
    mov rax, 0x03
    mov rdi, [file_fd]
    lea rsi, [rel file_buffer]
    mov rdx, 0x100
    int 0x80

    mov [bytes_read], rax

    ; close file descriptor
    mov rax, 0x0A
    mov rdi, [file_fd]
    int 0x80

    cmp qword [bytes_read], 0x00
    je final

    ; print file contents
    mov rax, 0x02
    mov rdi, 0x01
    lea rsi, [rel file_buffer]
    mov rdx, [bytes_read]
    int 0x80

    ; print trailing newline
    mov rax, 0x02
    mov rdi, 0x01
    lea rsi, [rel newline]
    mov rdx, newline_len
    int 0x80

    jmp final

open_error:
    mov rax, 0x02
    mov rdi, 0x01
    lea rsi, [rel open_error_msg]
    mov rdx, open_error_msg_len
    int 0x80

final:
    ; exit syscall
    mov rax, 0x01
    int 0x80

    jmp $

section .data
    prompt db "Path: "
    prompt_len equ $ - prompt

    open_error_msg db "cat: could not open file", 0xA
    open_error_msg_len equ $ - open_error_msg

    newline db 0x0A
    newline_len equ $ - newline

    char_buffer db 0x00
    path_len dq 0x00
    file_fd dq 0x00
    bytes_read dq 0x00
