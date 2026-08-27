// syscall 4 - execute an ELF binary from a path

use alloc::{format, vec::Vec};

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    elf,
    io::LogType,
    log, scheduling,
    scheduling::process::Process,
    syscall::{Errno, SyscallResult},
};

/// Maximum byte length of the argument blob.
const ARGS_MAX_BYTES: usize = 4096;

/// Maximum number of process arguments, including the program name.
const ARGS_MAX_COUNT: usize = 64;

/// Splits a NUL-separated argument blob into its entries.
///
/// The blob holds `count` entries laid end to end, each one NUL-terminated,
/// so an argument may contain spaces or quotes. That is the whole reason the
/// ABI is a blob rather than a string.
///
/// ## Arguments
///
/// - `blob` the raw argument bytes copied out of the caller
/// - `count` how many entries the caller says the blob holds
///
/// ## Returns
/// The entries, or `None` when the blob does not hold exactly `count`
/// NUL-terminated pieces or any of them is not valid UTF-8.
fn split_args_blob(blob: &[u8], count: usize) -> Option<Vec<&str>> {
    let mut args = Vec::new();
    let mut start = 0;

    for (index, byte) in blob.iter().enumerate() {
        if *byte != 0 {
            continue;
        }

        // stop rather than collecting a 65th entry, the count was checked
        // before this ran
        if args.len() == count {
            return None;
        }

        args.push(core::str::from_utf8(&blob[start..index]).ok()?);
        start = index + 1;
    }

    // trailing bytes with no NUL mean the blob and the count disagree, which
    // is exactly the case that would otherwise read past the last argument
    if start != blob.len() || args.len() != count {
        return None;
    }

    Some(args)
}

pub fn execute(stack: &FullInterruptStackFrame) -> SyscallResult {
    let buffer_addr = stack.rdi;
    let buffer_size = stack.rsi;
    let args_addr = stack.rdx;
    let args_size = stack.r10;
    let args_count = stack.r8;

    let Some(page_table) = scheduling::get_current_process_page_table() else {
        log!(
            LogType::ERR,
            "execute: no current process page table, rdi: 0x{:X}, rsi: 0x{:X}",
            buffer_addr,
            buffer_size
        );

        return Some(Err(Errno::Srch));
    };

    let Some(buffer) = Process::copy_from_user(&page_table, buffer_addr, buffer_size) else {
        log!(
            LogType::ERR,
            "execute: failed to copy path from user pointer, rdi: 0x{:X}, rsi: 0x{:X}",
            buffer_addr,
            buffer_size
        );

        return Some(Err(Errno::Fault));
    };

    let path = match core::str::from_utf8(&buffer) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!(
                "Invalid string for execute syscall, rdi: 0x{:X}, rsi: 0x{:X}\n",
                buffer_addr, buffer_size
            );

            log!(LogType::ERR, "{}\n{:?}", msg, e);
            return Some(Err(Errno::Inval));
        }
    };

    if path.rsplit('/').next() == Some("shell.elf") {
        log!(LogType::ERR, "execute: blocked attempt to launch shell.elf");
        return Some(Err(Errno::Perm));
    }

    if args_size > ARGS_MAX_BYTES {
        log!(
            LogType::ERR,
            "execute: argument blob too long, rdx: 0x{:X}, r10: 0x{:X}",
            args_addr,
            args_size
        );

        return Some(Err(Errno::Range));
    }

    // checked before the blob is parsed so a huge count cannot drive the
    // allocation below it
    if args_count > ARGS_MAX_COUNT {
        log!(
            LogType::ERR,
            "execute: too many arguments, count: {}",
            args_count
        );

        return Some(Err(Errno::Range));
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

            return Some(Err(Errno::Fault));
        };

        buffer
    };

    let mut argv: Vec<&str> = if args_count == 0 {
        // no argv at all, give the program the conventional argv[0] rather
        // than an empty vector it cannot make sense of
        alloc::vec![path]
    } else {
        let Some(args) = split_args_blob(&args_buffer, args_count) else {
            log!(
                LogType::ERR,
                "execute: malformed argument blob, rdx: 0x{:X}, r10: 0x{:X}, count: {}",
                args_addr,
                args_size,
                args_count
            );

            return Some(Err(Errno::Inval));
        };

        args
    };

    // a caller that supplies argv chooses argv[0] as well, the way execve
    // does, but an empty first entry would leave the program nameless
    if argv[0].is_empty() {
        argv[0] = path;
    }

    let file = scheduling::find_file_from_path(path);

    let Some(file) = file else {
        return Some(Err(Errno::NoEnt));
    };

    // read file
    let region = {
        let file_guard = file.read();
        let file_name = file_guard.name();
        let Some(region) = file_guard.read() else {
            log!(LogType::ERR, "execute: failed to read file {:?}", file_name);
            return Some(Err(Errno::Io));
        };

        region
    };

    // the child inherits the parent environment, the entries come from the
    // kernel rather than a user pointer so there is nothing to validate
    let env = scheduling::current_env();
    let envp: Vec<&str> = env.iter().map(|entry| entry.as_str()).collect();

    let Some(elf_entry) = elf::load(region, &argv, &envp) else {
        log!(
            LogType::ERR,
            "execute: elf::load failed for path {:?}",
            path
        );

        return Some(Err(Errno::NoExec));
    };

    let pid = scheduling::deploy(elf_entry, true);
    if pid == 0 {
        // pids start at 1, so deploy only answers with zero when it failed
        return Some(Err(Errno::NoMem));
    }

    Some(Ok(pid))
}
