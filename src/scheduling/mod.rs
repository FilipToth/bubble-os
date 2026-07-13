use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{sync::Arc, vec::Vec};
use process::{FileDescriptor, Process, ProcessEntry};
use spin::{Mutex, RwLock};

use crate::log;
use crate::{
    arch::x86_64::{gdt::GDT, registers::FullInterruptStackFrame},
    elf,
    fs::fs::{normalize_path_components, Directory, File},
    io::LogType,
    mem::{paging::PageTable, GLOBAL_MEMORY_CONTROLLER},
    print, with_root_dir,
};

pub mod process;

pub static SCHEDULING_ENABLED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
pub static PID_COUNTER: AtomicUsize = AtomicUsize::new(0);

unsafe fn jump(context: &FullInterruptStackFrame) {
    let ctx_addr = context as *const FullInterruptStackFrame as usize;

    core::arch::asm!(
        "push {ss}",
        "push {rsp}",
        "push {rflags}",
        "push {cs}",
        "push {rip}",
        "push [{ctx} + 0x00]", // r8
        "push [{ctx} + 0x08]", // r9
        "push [{ctx} + 0x10]", // r10
        "push [{ctx} + 0x18]", // r11
        "push [{ctx} + 0x20]", // r12
        "push [{ctx} + 0x28]", // r13
        "push [{ctx} + 0x30]", // r14
        "push [{ctx} + 0x38]", // r15
        "push [{ctx} + 0x40]", // rbp
        "push [{ctx} + 0x48]", // rdi
        "push [{ctx} + 0x50]", // rsi
        "push [{ctx} + 0x58]", // rdx
        "push [{ctx} + 0x60]", // rcx
        "push [{ctx} + 0x68]", // rbx
        "push [{ctx} + 0x70]", // rax

        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",

        "iretq",

        ss = in(reg) context.ss,
        rsp = in(reg) context.rsp,
        rflags = in(reg) context.rflags,
        cs = in(reg) context.cs,
        rip = in(reg) context.rip,
        ctx = in(reg) ctx_addr,
        options(noreturn)
    );
}

fn next_process(interrupt_stack: Option<&FullInterruptStackFrame>) -> Option<Process> {
    let mut current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let mut processes = PROCESSES.lock();
    let processes_len = processes.len();

    if processes_len == 0 {
        return None;
    }

    if let Some(interrupt_stack) = interrupt_stack {
        match processes.get_mut(current_index) {
            Some(current) => {
                // the bug happens when accessing anything from current after we call the exit syscall

                // Avoid saving kernel
                let _m = current.pid + 1;
                let is_not_presched = !current.pre_schedule;
                let rip = interrupt_stack.rip;

                if is_not_presched && rip > 0x1FFFFF {
                    // save current context
                    current.context = interrupt_stack.clone();
                }
            }
            None => {
                log!(
                    LogType::ERR,
                    "schedule: current index {} out of bounds, process count {}",
                    current_index,
                    processes_len
                );
            }
        };
    }

    let mut passes = 0;
    loop {
        current_index = if current_index + 1 >= processes_len {
            0
        } else {
            current_index + 1
        };

        let (blocking, awaiting_process) = {
            let process = &mut processes[current_index];
            (process.blocking, process.awaiting_process)
        };

        let mut new_current_ready = !blocking;
        if let Some(subprocess_pid) = awaiting_process {
            let process_found = processes.iter().any(|p| p.pid == subprocess_pid);
            new_current_ready = !process_found;
        }

        if new_current_ready {
            CURRENT_INDEX.store(current_index, Ordering::SeqCst);

            let new_current = &mut processes[current_index];
            new_current.pre_schedule = false;
            new_current.awaiting_process = None;

            return Some(new_current.clone());
        }

        passes += 1;
        if passes >= processes_len {
            return None;
        }
    }
}

pub fn schedule(interrupt_stack: Option<&FullInterruptStackFrame>) {
    let process_to_jump = match next_process(interrupt_stack) {
        Some(p) => p,
        None => {
            unsafe { core::arch::asm!("sti") };
            loop {}
        }
    };

    // switch to user page table
    {
        let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
        let Some(mc) = mc.as_mut() else {
            log!(
                LogType::ERR,
                "schedule: memory controller is not initialized"
            );

            unsafe { core::arch::asm!("sti") };
            loop {}
        };

        let Some(ring3_page_table) = process_to_jump.ring3_page_table else {
            log!(
                LogType::ERR,
                "schedule: pid {} has no ring3 page table",
                process_to_jump.pid
            );

            unsafe { core::arch::asm!("sti") };
            loop {}
        };

        if mc.switch_table(&ring3_page_table).is_none() {
            log!(
                LogType::ERR,
                "schedule: failed to switch to pid {} page table 0x{:X}",
                process_to_jump.pid,
                ring3_page_table.addr
            );

            unsafe { core::arch::asm!("sti") };
            loop {}
        }

        // drop memory controller ref
        // and kernel page table ref
    };

    unsafe { jump(&process_to_jump.context) };
}

pub fn deploy(entry: ProcessEntry, fork_current: bool) -> usize {
    let pid = PID_COUNTER.load(Ordering::SeqCst);
    PID_COUNTER.store(pid + 1, Ordering::SeqCst);

    let mut processes = PROCESSES.lock();
    let parent_state = if fork_current && processes.len() != 0 {
        // basically fork the cwd from calling process
        let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
        let Some(current) = processes.get(current_index) else {
            log!(
                LogType::ERR,
                "deploy: current index {} out of bounds while forking pid {}, process count {}",
                current_index,
                pid,
                processes.len()
            );

            return 0;
        };

        Some((current.curr_working_dir.clone(), current.fd_table.clone()))
    } else {
        None
    };

    let cwd = if let Some((cwd, _)) = &parent_state {
        cwd.clone()
    } else {
        // root directory
        with_root_dir!(root, { root })
    };

    let Some(mut process) = Process::from(entry, pid, cwd) else {
        log!(
            LogType::ERR,
            "deploy: failed to construct process pid {}",
            pid
        );

        return 0;
    };

    if let Some((_, fd_table)) = parent_state {
        process.fd_table = fd_table;
    }

    let cs = GDT.1.user_code.0;
    let ss = GDT.1.user_data.0;

    process.context.cs = cs as usize;
    process.context.ss = ss as usize;
    process.context.rflags = 0x202;
    process.context.rsp = process.stack.top;

    processes.push(process);

    pid
}

pub fn block_current() {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current = &mut processes[current_index];
    current.blocking = true;
}

pub fn process_input(input: char) {
    let mut processes = PROCESSES.lock();
    for process in processes.iter_mut() {
        if !process.blocking {
            continue;
        }

        // process is awaiting input
        process.context.rax = input as usize;
        process.blocking = false;
    }
}

pub fn current_wait_for_process(subprocess: usize) {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        log!(
            LogType::ERR,
            "wait_for_process: no processes while waiting for pid {}",
            subprocess
        );

        return;
    }

    if current_index >= processes.len() {
        log!(
            LogType::ERR,
            "wait_for_process: current index {} out of bounds, process count {}, waiting for pid {}",
            current_index,
            processes.len(),
            subprocess
        );

        return;
    }

    // TODO: Check if subprocess PID exists
    let current = &mut processes[current_index];
    current.awaiting_process = Some(subprocess);
}

pub fn exit_current() {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    if processes.len() == 0 {
        log!(LogType::ERR, "exit_current: no processes to exit");
        return;
    }

    if current_index >= processes.len() {
        log!(
            LogType::ERR,
            "exit_current: current index {} out of bounds, process count {}",
            current_index,
            processes.len()
        );

        return;
    }

    let removed = processes.remove(current_index);

    elf::unmap(&removed.start_region);

    {
        let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
        if let Some(mc) = mc.as_mut() {
            mc.free_stack(&removed.stack);

            if let Some(page_table) = &removed.ring3_page_table {
                page_table.free_user_subtables(&mut mc.slot_allocator, &mut mc.temp_mapper);
                mc.slot_allocator.free(page_table.addr);
            } else {
                log!(
                    LogType::ERR,
                    "exit_current: pid {} has no ring3 page table to free",
                    removed.pid
                );
            }
        } else {
            log!(
                LogType::ERR,
                "exit_current: memory controller is not initialized while freeing pid {} stack",
                removed.pid
            );
        }
    }

    // adjust current process index
    let new_index = if current_index != 0 {
        current_index - 1
    } else {
        0
    };

    CURRENT_INDEX.store(new_index, Ordering::SeqCst);
}

pub fn get_current_cwd() -> Arc<dyn Directory> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        with_root_dir!(root, { root })
    } else {
        let current_process = &mut processes[current_index];
        let cwd = &current_process.curr_working_dir;

        cwd.clone()
    }
}

pub fn get_current_process_page_table() -> Option<PageTable> {
    let processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get(current_index)?;

    current_process.ring3_page_table.clone()
}

pub fn get_current_file_descriptor(fd: usize) -> Option<FileDescriptor> {
    let processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get(current_index)?;

    current_process.get_fd(fd).cloned()
}

/// Finds a file from either an absolute path or the current process cwd.
///
/// ## Arguments
///
/// - `path` the file path to resolve
///
/// ## Returns
/// The file if it exists.
pub fn find_file_from_path(path: &str) -> Option<Arc<RwLock<dyn File>>> {
    if let Some(path) = path.strip_prefix("~/") {
        with_root_dir!(root, {
            let components = normalize_path_components(path);
            root.find_file_components(&components)
        })
    } else if let Some(path) = path.strip_prefix('/') {
        with_root_dir!(root, {
            let components = normalize_path_components(path);
            root.find_file_components(&components)
        })
    } else {
        let cwd = get_current_cwd();
        let components = normalize_path_components(path);
        cwd.find_file_components(&components)
    }
}

/// Finds a directory from either an absolute path or the current process cwd.
///
/// ## Arguments
///
/// - `path` the directory path to resolve
///
/// ## Returns
/// The directory if it exists.
pub fn find_directory_from_path(path: &str) -> Option<Arc<dyn Directory>> {
    if path == "/" || path == "~" {
        with_root_dir!(root, {
            let root: Arc<dyn Directory> = root;
            Some(root)
        })
    } else if let Some(path) = path.strip_prefix("~/") {
        with_root_dir!(root, {
            let components = normalize_path_components(path);
            if components.is_empty() {
                let root: Arc<dyn Directory> = root;
                Some(root)
            } else {
                root.find_directory_components(&components)
            }
        })
    } else if let Some(path) = path.strip_prefix('/') {
        with_root_dir!(root, {
            let components = normalize_path_components(path);
            if components.is_empty() {
                let root: Arc<dyn Directory> = root;
                Some(root)
            } else {
                root.find_directory_components(&components)
            }
        })
    } else {
        let cwd = get_current_cwd();
        let components = normalize_path_components(path);
        if components.is_empty() {
            Some(cwd)
        } else {
            cwd.find_directory_components(&components)
        }
    }
}

pub fn curr_process_open_file(path: &str, readable: bool, writable: bool) -> Option<usize> {
    let file = find_file_from_path(path)?;

    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get_mut(current_index)?;

    Some(current_process.open_file(file, readable, writable))
}

pub fn close_current_file_descriptor(fd: usize) -> bool {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let Some(current_process) = processes.get_mut(current_index) else {
        return false;
    };

    current_process.close_fd(fd)
}

pub fn read_current_file_descriptor(fd: usize, size: usize) -> Option<Vec<u8>> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get_mut(current_index)?;

    current_process.read_fd(fd, size)
}

pub fn write_current_file_descriptor(fd: usize, bytes: &[u8]) -> Option<usize> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get_mut(current_index)?;

    current_process.write_fd(fd, bytes)
}

pub fn change_cwd(cwd: Arc<dyn Directory + Send + Sync>) {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current_process = &mut processes[current_index];
    print!(
        "Saving cwd: {}, to index: {}, pid: {}\n",
        cwd.name(),
        current_index,
        current_process.pid
    );

    current_process.curr_working_dir = cwd.clone();
}

pub fn enable() {
    log!(crate::io::LogType::SCHED, "Enabled Scheduling!");
    SCHEDULING_ENABLED.store(true, Ordering::SeqCst);
}
