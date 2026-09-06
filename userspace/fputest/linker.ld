ENTRY(_start)

/* One LOAD segment per protection level. Without an explicit PHDRS block
   the linker merges every section into a single RWX segment, and the
   kernel then has no choice but to map the program's code writable. */
PHDRS {
    text   PT_LOAD FLAGS(5);    /* r-x */
    rodata PT_LOAD FLAGS(4);    /* r-- */
    data   PT_LOAD FLAGS(6);    /* rw- */
}

SECTIONS {
    . = 0x0000700040000000;

    .text : {
        *(.text*)
    } :text

    /* the kernel maps whole pages, so two segments sharing a page would
       give the more permissive one's flags to both */
    . = ALIGN(0x1000);

    .rodata : {
        *(.rodata*)
    } :rodata

    . = ALIGN(0x1000);

    .data : {
        *(.data*)
    } :data

    .bss : {
        *(.bss*)
        *(COMMON)
    } :data
}
