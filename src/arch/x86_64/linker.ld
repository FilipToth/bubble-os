ENTRY(start)

SECTIONS {
    . = 1M;

    .boot : {
        /* The multiboot header must be at the beginning */
        KEEP(*(.header))
    }

    .text : {
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data.rel.ro : {
        *(.data.rel.ro.local*) *(.data.rel.ro .data.rel.ro.*)
    }
}