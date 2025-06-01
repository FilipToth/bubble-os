; just make syscall that reads the inputs of a directory
; takes in a buffer pointer, and max amount of item entries
; to read from the directory. And a path pointer, with a
; path buffer size. Then returns the number of dir entries
; read.

struc DirEntry
    .name:      resb 12
    .is_dir:    resb 1
    ._resv:     resb 3
    .size:      resd 1
endstruc

section .bss
entry_buffer: resb DirEntry_size
