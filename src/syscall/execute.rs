// syscall 4 - execute an ELF binary from a path

use alloc::{format, vec::Vec};

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame, elf, io::LogType, log, scheduling,
    scheduling::process::Process,
};

/// Maximum byte length of the argument string.
const ARGS_MAX_BYTES: usize = 4096;

/// Maximum number of process arguments, including the program name.
const ARGS_MAX_COUNT: usize = 64;

pub fn execute(stack: &FullInterruptStackFrame) -> Option<usize> {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;
    let args_addr = stack.rdx;
    let args_size = stack.r10;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        log!(
            LogType::ERR,
            "execute: no current process page table, rdi: 0x{:X}, rsi: 0x{:X}",
            buffer_addr,
            buffer_size
        );

        return Some(0);
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        log!(
            LogType::ERR,
            "execute: failed to copy path from user pointer, rdi: 0x{:X}, rsi: 0x{:X}",
            buffer_addr,
            buffer_size
        );

        return Some(0);
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for execute syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            log!(LogType::ERR, "{}\n{:?}", msg, e);
            return Some(0);
        }
    };

    if path.rsplit('/').next() == Some("shell.elf") {
        log!(LogType::ERR, "execute: blocked attempt to launch shell.elf");
        return Some(0);
    }

    if args_size > ARGS_MAX_BYTES {
        log!(
            LogType::ERR,
            "execute: argument string too long, rdx: 0x{:X}, r10: 0x{:X}",
            args_addr,
            args_size
        );

        return Some(0);
    }

    let args_buffer = if args_size == 0 {
        Vec::new()
    } else {
        let Some(buffer) = Process::copy_from_user(&page_table, args_addr, args_size) else {
            log!(
                LogType::ERR,
                "execute: failed to copy arguments from user pointer, rdx: 0x{:X}, r10: 0x{:X}",
                args_addr,
                args_size
            );

            return Some(0);
        };

        buffer
    };

    let Ok(args) = core::str::from_utf8(&args_buffer) else {
        log!(
            LogType::ERR,
            "execute: invalid argument string, rdx: 0x{:X}, r10: 0x{:X}",
            args_addr,
            args_size
        );

        return Some(0);
    };

    // the program name is argv[0], the argument string
    // provides the rest
    let mut argv: Vec<&str> = Vec::new();
    argv.push(path);
    argv.extend(args.split_ascii_whitespace());

    if argv.len() > ARGS_MAX_COUNT {
        log!(
            LogType::ERR,
            "execute: too many arguments, count: {}",
            argv.len()
        );

        return Some(0);
    }

    let file = scheduling::find_file_from_path(path);

    let Some(file) = file else {
        return Some(0);
    };

    // read file
    let region = {
        let file_guard = file.read();
        let file_name = file_guard.name();
        let Some(region) = file_guard.read() else {
            log!(LogType::ERR, "execute: failed to read file {:?}", file_name);
            return Some(0);
        };

        region
    };

    let Some(elf_entry) = elf::load(region, &argv) else {
        log!(
            LogType::ERR,
            "execute: elf::load failed for path {:?}",
            path
        );

        return Some(0);
    };

    let pid = scheduling::deploy(elf_entry, true);

    Some(pid)
}
