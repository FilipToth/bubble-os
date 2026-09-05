ENTRY(_start)

/* Same three-segment layout the Rust programs use: one LOAD segment per
   protection level, so the kernel never has to map code writable. */
PHDRS {
    text   PT_LOAD FLAGS(5);    /* r-x */
    rodata PT_LOAD FLAGS(4);    /* r-- */
    data   PT_LOAD FLAGS(6);    /* rw- */
}

SECTIONS {
    . = 0x0000700040000000;

    .text : {
        *(.text.unlikely* .text.hot* .text*)
    } :text

    /* the kernel maps whole pages, so two segments sharing a page would
       give the more permissive one's flags to both */
    . = ALIGN(0x1000);

    .rodata : {
        *(.rodata*)

        /* gcc emits these for C even without exceptions. Nothing unwinds
           yet, but they have to land somewhere or ld warns about orphans */
        *(.eh_frame*)
        *(.gcc_except_table*)
    } :rodata

    . = ALIGN(0x1000);

    .data : {
        *(.data.rel.ro*)
        *(.got .got.plt)
        *(.data*)
    } :data

    /* Walked by newlib's __libc_init_array, which crt0.S calls before main.
       KEEP stops --gc-sections from dropping constructors, which are only
       ever reached through these arrays and so look unreferenced. */
    .preinit_array : {
        PROVIDE_HIDDEN(__preinit_array_start = .);
        KEEP(*(.preinit_array))
        PROVIDE_HIDDEN(__preinit_array_end = .);
    } :data

    .init_array : {
        PROVIDE_HIDDEN(__init_array_start = .);
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
        KEEP(*(.init_array))
        PROVIDE_HIDDEN(__init_array_end = .);
    } :data

    .fini_array : {
        PROVIDE_HIDDEN(__fini_array_start = .);
        KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)))
        KEEP(*(.fini_array))
        PROVIDE_HIDDEN(__fini_array_end = .);
    } :data

    .bss : {
        *(.bss*)
        *(COMMON)
    } :data

    /DISCARD/ : {
        *(.comment)
        *(.note*)
    }
}
