bits 64
default rel

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

    ; display welcome message
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, welcome_msg
    mov rdx, welcome_msg_len
    int 0x80

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
    je evaluate_command

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

evaluate_command:
    ; print newline
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, newline,
    mov rdx, newline_len
    int 0x80

    ; check if input is empty
    mov rax, 0x00
    cmp [input_counter], rax
    je reset_after_input

check_builtins:
    ; check for cd
    cmp word [input_buffer], "cd"
    jne execute_elf
    cmp byte [input_buffer + 0x02], ' '
    je execute_cd

execute_elf:
    ; try call execute syscall on input buffer
    mov rax, 0x04
    mov rdi, input_buffer
    mov rsi, [input_counter]
    int 0x80

    ; PID will be in rax, check if we
    ; actually ran the ELF
    cmp rax, 0x00
    je error

    ; wait for subprocess completion
    mov rdi, rax
    mov rax, 0x06
    int 0x80

    jmp reset_after_input

error:
    ; print error message
    mov rax, 0x02
    mov rdi, 0x01,
    mov rsi, err_msg
    mov rdx, err_msg_len
    int 0x80

reset_after_input:
    ; reset buffer pointer
    mov rdi, 0x00
    mov [input_counter], rdi

    jmp mainloop


; =============================
;   CHANGE DIRECTORY BUILTIN
; =============================
execute_cd:
    ; call cd syscall
    mov rax, 0x08
    mov rdi, input_buffer + 0x02
    mov rsi, [input_counter]
    sub rsi, 0x02
    int 0x80

    jmp reset_after_input


section .data
    welcome_msg dw 0xA, "Welcome to the Bubble OS Kernel Shell :D", 0xA, 0xA
    welcome_msg_len equ $ - welcome_msg

    err_msg dw "Program or command not found...", 0xA
    err_msg_len equ $ - err_msg

    preface dw "$", 0x20
    preface_len equ $ - preface

    newline dw 0xA
    newline_len equ $ - newline

    input_counter dw 0x00