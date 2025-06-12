struc DirEntry
    .name:      resb 11
    .attr:      resb 1
    .size:      resd 4
endstruc

section .bss
    stack_bottom:
        resb 4096
    stack_top:

    entry_buffer: resb 10 * DirEntry_size
    curr_entry_ptr: resb 1

align 0x08

section .text
    global _start

_start:
    mov rax, stack_top
    mov rsp, rax

    ; read dir syscall
    mov rax, 0x07
    mov rdi, 0x00
    mov rsi, 0x00,
    mov rdx, entry_buffer
    mov rcx, 0x0A
    int 0x80

    ; number of entries now in rax
    mov byte [num_entries], al

    ; initialize entry ptr
    mov rcx, entry_buffer
    mov [curr_entry_ptr], rcx

iterate_entries:
    ; print current entry
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, [curr_entry_ptr]
    mov rdx, 0x0B
    int 0x80

    ; print newline
    mov rax, 0x02
    mov rdi, 0x01
    mov rsi, newline
    mov rdx, 0x01
    int 0x80

    ; increment pointer
    ; TODO: use some struct sizeof function
    mov rax, [curr_entry_ptr]
    add rax, 0x10
    mov [curr_entry_ptr], rax 

    ; check if end of entries
    mov al, byte [num_entries]
    cmp [num_entries_read], al
    jae final

    ; increment counter
    mov rax, [num_entries_read]
    add rax, 0x01
    mov [num_entries_read], rax
 
    jmp iterate_entries

final:
    ; exit syscall
    mov rax, 0x01
    int 0x80

    jmp $

section .data
    char_buffer dw 0x00
    newline db 0x0A

    num_entries db 0x00
    num_entries_read db 0x00
