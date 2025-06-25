section .bss
    stack_bottom:
        resb 4096
    stack_top:

    input_buffer: resb 128
    buffer_ptr: resb 1

align 0x08

section .text
    global _start

_start:
    mov rax, stack_top
    mov rsp, rax

    ; initialize buffer ptr
    lea rax, [input_buffer]
    mov [buffer_ptr], rax

mainloop:
    ; print shell command preface
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, preface
    mov rdx, preface_len
    int 0x80

input_loop:
    ; TODO: Check for buffer length

    ; wait for user input
    mov rax, 0x03
    int 0x80

    ; check if char is enter
    cmp rax, 0x0D
    je perform_cd

    ; user input char is now in rax
    ; append input char into buffer
    mov rcx, [buffer_ptr]
    movzx rdx, byte [input_counter]
    mov [rcx, rdx], rax

    ; increase counter
    mov r11, 0x01
    mov rax, [input_counter]
    add rax, r11
    mov [input_counter], rax

    ; create addr pointer to new char
    add rcx, rdx

    ; print character to screen
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, rcx
    mov rdx, 0x01
    int 0x80

    jmp input_loop

perform_cd:
    ; check if input is empty
    mov rax, 0x00
    cmp [input_counter], rax
    je input_loop

    ; call cd syscall on input buffer
    mov rax, 0x08
    mov rdi, input_buffer
    mov rsi, [input_counter]
    int 0x80

    ; print newline
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, newline,
    mov rdx, newline_len
    int 0x80

    ; exit syscall
    mov rax, 0x01
    int 0x80

    jmp $

section .data
    preface dw "dir name: ", 0x20
    preface_len equ $ - preface

    newline dw 0xA
    newline_len equ $ - newline

    input_counter dw 0x00