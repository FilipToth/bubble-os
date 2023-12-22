section .header

header_start:
    dd 0xe85250d6                                                   ; magic
    dd 0                                                            ; 0 for i386, 4 for mips
    dd header_end - header_start                                    ; length of the header
    dd 0x100000000 - (0xe85250d6 + 0 + (header_end - header_start)) ; checksum

    ; optional header tags go here

    dw 0    ; type
    dw 0    ; flags
    dd 0    ; size
header_end:
