global kernel_start

kernel_start:
    extern rust_main
    call rust_main

    ; if kernel returns, halt
    hlt