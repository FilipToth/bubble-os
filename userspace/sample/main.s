section .bss
    resb 4096

section .text
    global _start

_start:
    mov esp, stack_top
    int 0x70
    hlt

section .bss
stack_bottom:
    resb 4096
stack_top: