global kernel_start

kernel_start:
    extern rust_main_test
    call rust_main_test

    ; if tests return, halt
    hlt