section .bss
    resb 4096

section .text
    global _start

_start:
    mov esp, stack_top
    int 0x34

    mov eax, 0x01
    int 0x80

    hlt

section .bss
stack_bottom:
    resb 4096
stack_top: