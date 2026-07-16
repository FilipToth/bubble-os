#![no_std]
#![no_main]

use core::{arch::global_asm, cell::UnsafeCell, panic::PanicInfo};

use ulib::DirEntry;

const MAX_ENTRIES: usize = 32;

global_asm!(
    r#"
    .section .bss
    .align 16

stack_bottom:
    .skip 4096

stack_top:
    .section .text
    .global _start

_start:
    lea rsp, [rip + stack_top]
    call rust_main

    mov rax, 1
    int 0x80

1:
    jmp 1b
"#
);

// entries are too large for the 4 KiB stack, so keep them in .bss
struct EntryBuffer(UnsafeCell<[DirEntry; MAX_ENTRIES]>);

unsafe impl Sync for EntryBuffer {}

static ENTRY_BUFFER: EntryBuffer = EntryBuffer(UnsafeCell::new([DirEntry::empty(); MAX_ENTRIES]));

#[no_mangle]
extern "C" fn rust_main() -> ! {
    let entries = unsafe { &mut *ENTRY_BUFFER.0.get() };
    let count = ulib::read_dir(entries);

    for entry in &entries[..count] {
        if entry.is_directory() {
            ulib::stdout(b"[ DIR ] ");
        } else {
            ulib::stdout(b"        ");
        }

        ulib::stdout(entry.name_bytes());
        ulib::stdout(b"\n");
    }

    ulib::exit();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ulib::exit();
}
