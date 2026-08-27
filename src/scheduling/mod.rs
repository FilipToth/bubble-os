use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use process::{FileDescriptor, Process, ProcessEntry};
use spin::{Mutex, RwLock};

use crate::log;
use crate::{
    arch::x86_64::{gdt::GDT, registers::FullInterruptStackFrame},
    elf,
    fs::fs::{
        normalize_path_components, Directory, File, FileStat, O_ACCMODE, O_APPEND, O_CREAT,
        O_EXCL, O_RDONLY, O_RDWR, O_SUPPORTED, O_TRUNC, O_WRONLY,
    },
    io::LogType,
    mem::{
        paging::{entry::EntryFlags, Page, PageTable},
        MemoryController, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE,
    },
    print,
    syscall::Errno,
    time, with_root_dir,
};
use x86_64::instructions::tlb;

pub mod process;

pub static SCHEDULING_ENABLED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
/// The next pid to hand out.
///
/// Starts at 1 so that pid 0 is never a real process. Userspace treats it as
/// a reserved sentinel, and a wait on it is an unambiguous ESRCH rather than
/// a wait on whichever process happened to boot first.
pub static PID_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Exit statuses of processes that have already exited, keyed by pid.
///
/// A record is written when a process exits and taken by whoever waits for
/// it. Nothing reaps the record of a process nobody ever waits for, so the
/// map is capped and the oldest record is dropped on overflow.
///
/// `PID_COUNTER` never reuses a pid, so the BTree key order is also the
/// insertion order and the first key is always the oldest record. Recycling
/// pids would break that, and the eviction would start dropping arbitrary
/// records instead of the stalest ones.
///
/// Lock ordering: `PROCESSES` is always taken before `EXIT_RECORDS`, never
/// the other way around.
static EXIT_RECORDS: Mutex<BTreeMap<usize, usize>> = Mutex::new(BTreeMap::new());

const EXIT_RECORDS_MAX: usize = 64;

/// The environment the first process starts with. Every other process
/// inherits its parent's environment instead, so this is only ever read on
/// the boot path and by a non-forking deploy.
pub const DEFAULT_ENV: [&str; 2] = ["PATH=/bin", "HOME=/"];

/// The furthest a process may push its program break past `heap_start`.
///
/// `brk` is the one syscall that lets ring 3 decide how many frames the kernel
/// hands out, so the size needs a ceiling that is not "all of physical memory".
const MAX_HEAP_SIZE: usize = 64 * 1024 * 1024;

/// Heap pages are writable data, never code. Leaving out `NO_EXECUTE` here
/// would hand every process a writable executable mapping and undo the W^X
/// split the ELF loader applies to the segments.
const HEAP_FLAGS: EntryFlags = EntryFlags::WRITABLE
    .union(EntryFlags::RING3_ACCESSIBLE)
    .union(EntryFlags::NO_EXECUTE);

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

        let (blocking, awaiting_process, sleep_until_tick) = {
            let process = &mut processes[current_index];
            (
                process.blocking,
                process.awaiting_process,
                process.sleep_until_tick,
            )
        };

        let mut new_current_ready = !blocking;
        let mut exit_status = None;

        if let Some(subprocess_pid) = awaiting_process {
            let process_found = processes.iter().any(|p| p.pid == subprocess_pid);
            new_current_ready = !process_found;

            if new_current_ready {
                // the record is only taken when this process is about to be
                // picked, a candidate that is skipped must not consume it
                exit_status = take_exit_status(subprocess_pid);
            }
        }

        if let Some(deadline) = sleep_until_tick {
            if time::current_ticks() < deadline {
                new_current_ready = false;
            }
        }

        if new_current_ready {
            CURRENT_INDEX.store(current_index, Ordering::SeqCst);

            let new_current = &mut processes[current_index];
            new_current.pre_schedule = false;
            new_current.awaiting_process = None;
            new_current.sleep_until_tick = None;

            if let Some(status) = exit_status {
                // the process is resuming inside the wait_for_process
                // syscall, hand the child status back as its return value.
                // Statuses are masked to a byte at exit, so one can never be
                // mistaken for a negative errno
                new_current.context.rax = crate::syscall::encode(Ok(status));
            }

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

        Some((
            current.curr_working_dir.clone(),
            current.fd_table.clone(),
            current.env.clone(),
        ))
    } else {
        None
    };

    let cwd = if let Some((cwd, _, _)) = &parent_state {
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

    match parent_state {
        Some((_, fd_table, env)) => {
            process.fd_table = fd_table;
            process.env = env;
        }
        None => {
            // the first process has no parent to inherit from
            process.env = DEFAULT_ENV.iter().map(|entry| String::from(*entry)).collect();
        }
    }

    let cs = GDT.1.user_code.0;
    let ss = GDT.1.user_data.0;

    process.context.cs = cs as usize;
    process.context.ss = ss as usize;
    process.context.rflags = 0x202;

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

/// Puts the current process to sleep until the tick counter reaches a
/// deadline.
///
/// The caller must yield to the scheduler afterwards for the sleep to take
/// effect.
///
/// ## Arguments
///
/// - `deadline_tick` the tick count at which the process becomes runnable
pub fn sleep_current_until(deadline_tick: u64) {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    if processes.len() == 0 {
        return;
    }

    let current = &mut processes[current_index];
    current.sleep_until_tick = Some(deadline_tick);
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

/// Stores the exit status of a process for whoever waits on it.
///
/// ## Arguments
///
/// - `pid` the pid of the process that exited
/// - `status` the status the process exited with
fn record_exit(pid: usize, status: usize) {
    let mut records = EXIT_RECORDS.lock();

    if records.len() >= EXIT_RECORDS_MAX && !records.contains_key(&pid) {
        // nobody is coming for the stalest record anymore
        records.pop_first();
    }

    records.insert(pid, status);
}

/// Takes the recorded exit status of a process that has already exited.
///
/// The record is consumed, a second wait for the same pid finds nothing.
///
/// ## Arguments
///
/// - `pid` the pid of the exited process
///
/// ## Returns
/// The exit status, or `None` when the process is still running or was
/// never recorded.
pub fn take_exit_status(pid: usize) -> Option<usize> {
    EXIT_RECORDS.lock().remove(&pid)
}

/// Terminates the currently scheduled process and frees everything it owns.
///
/// The caller must yield to the scheduler afterwards, the current process no
/// longer exists once this returns.
///
/// ## Arguments
///
/// - `status` the exit status handed to whoever waits for this process
pub fn exit_current(status: usize) {
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
    record_exit(removed.pid, status);

    elf::unmap(&removed.start_region);

    {
        let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
        if let Some(mc) = mc.as_mut() {
            // the heap only ever exists in this process' page table, which is
            // still the active one here, so it has to be given back now
            if let Some(heap_end) = heap_last_page(removed.heap_start, removed.heap_break) {
                release_heap_pages(mc, Page::for_address(removed.heap_start), heap_end);
                tlb::flush_all();
            }

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

/// The page holding the last byte a heap needs at a given break.
///
/// ## Arguments
///
/// - `heap_start` the lowest address the heap can occupy
/// - `program_break` the break to measure
///
/// ## Returns
/// `None` when the break sits at `heap_start`, where the heap needs no pages
/// at all.
fn heap_last_page(heap_start: usize, program_break: usize) -> Option<Page> {
    if program_break <= heap_start {
        return None;
    }

    Some(Page::for_address(program_break - 1))
}

/// Zeroes a heap range and returns its frames to the frame allocator.
///
/// The frames go straight back into the global pool, so they must not carry
/// the process' data into whoever is handed them next.
///
/// ## Arguments
///
/// - `mem_controller` the initialized memory controller
/// - `start` the first page to release
/// - `end` the last page to release, inclusive
fn release_heap_pages(mem_controller: &mut MemoryController, start: Page, end: Page) {
    let addr = start.start_address();
    let size = (end.start_address() + PAGE_SIZE) - addr;

    unsafe {
        core::ptr::write_bytes(addr as *mut u8, 0, size);
    }

    mem_controller.unmap(start, end);
}

/// Maps or releases heap pages until `[heap_start, new_break)` is exactly the
/// range backed by memory.
///
/// This works on the active page table, so the caller has to be running with
/// the owning process' table loaded.
///
/// ## Arguments
///
/// - `mem_controller` the initialized memory controller
/// - `heap_start` the lowest address the heap can occupy
/// - `current_break` the break the heap is at now
/// - `new_break` the break the heap should end up at
///
/// ## Returns
/// Whether the range is now backed. A growth that runs out of memory leaves
/// the heap exactly as it was.
fn resize_heap(
    mem_controller: &mut MemoryController,
    heap_start: usize,
    current_break: usize,
    new_break: usize,
) -> bool {
    let mapped_end = heap_last_page(heap_start, current_break);
    let wanted_end = heap_last_page(heap_start, new_break);

    // the break is tracked to the byte while memory is handed out by the page,
    // so a move that stays inside one page changes nothing that is mapped
    if mapped_end == wanted_end {
        return true;
    }

    let first_page = Page::for_address(heap_start);
    let resized = match (mapped_end, wanted_end) {
        (None, Some(wanted)) => mem_controller.try_map(first_page, wanted, HEAP_FLAGS),
        (Some(mapped), Some(wanted)) if wanted > mapped => {
            mem_controller.try_map(mapped + 1, wanted, HEAP_FLAGS)
        }
        (Some(mapped), Some(wanted)) => {
            release_heap_pages(mem_controller, wanted + 1, mapped);
            true
        }
        (Some(mapped), None) => {
            release_heap_pages(mem_controller, first_page, mapped);
            true
        }
        (None, None) => true,
    };

    // a released page stays reachable through its stale entry until something
    // reloads cr3, and a fresh one has to be visible on the return to ring 3
    tlb::flush_all();
    resized
}

/// Moves a process' program break, mapping or releasing pages to match it.
///
/// ## Arguments
///
/// - `process` the process whose break is moving, which has to be the one
/// whose page table is currently active
/// - `new_break` the requested break, which does not have to be page aligned
///
/// ## Returns
/// The new break, or the reason the request was refused.
fn set_process_break(process: &mut Process, new_break: usize) -> Result<usize, Errno> {
    // the heap can be given back down to its start, but never below it, the
    // ELF segments are down there. That is the caller asking for something
    // impossible rather than the kernel running out of memory
    if new_break < process.heap_start {
        return Err(Errno::Inval);
    }

    if new_break - process.heap_start > MAX_HEAP_SIZE {
        return Err(Errno::NoMem);
    }

    if new_break != process.heap_break {
        // PROCESSES is held by the caller, and it is always taken before the
        // memory controller
        let mut mem_controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let Some(mem_controller) = mem_controller.as_mut() else {
            log!(
                LogType::ERR,
                "set_process_break: memory controller is not initialized"
            );

            return Err(Errno::NoMem);
        };

        if !resize_heap(
            mem_controller,
            process.heap_start,
            process.heap_break,
            new_break,
        ) {
            return Err(Errno::NoMem);
        }
    }

    process.heap_break = new_break;
    Ok(new_break)
}

/// Moves the program break of the currently scheduled process to an absolute
/// address.
///
/// ## Arguments
///
/// - `new_break` the requested break
///
/// ## Returns
/// The new break, or the reason the request was refused.
pub fn current_set_break(new_break: usize) -> Result<usize, Errno> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let Some(process) = processes.get_mut(current_index) else {
        return Err(Errno::Srch);
    };

    set_process_break(process, new_break)
}

/// Moves the program break of the currently scheduled process by a signed
/// number of bytes.
///
/// Reading the break and moving it happen under the same lock, so an increment
/// can never be applied to a break that has changed in between.
///
/// ## Arguments
///
/// - `increment` how far to move the break, negative to give memory back
///
/// ## Returns
/// The new break, or the reason the request was refused.
pub fn current_adjust_break(increment: isize) -> Result<usize, Errno> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let Some(process) = processes.get_mut(current_index) else {
        return Err(Errno::Srch);
    };

    // an increment that runs off either end of the address space is the
    // caller's mistake, not a memory shortage
    let new_break = if increment >= 0 {
        process.heap_break.checked_add(increment as usize)
    } else {
        process.heap_break.checked_sub(increment.unsigned_abs())
    };

    let Some(new_break) = new_break else {
        return Err(Errno::Inval);
    };

    set_process_break(process, new_break)
}

/// Whether a process with this pid is currently alive.
///
/// A pid that has already exited is not alive, its status lives in the exit
/// records instead.
///
/// ## Arguments
///
/// - `pid` the pid to look for
pub fn process_exists(pid: usize) -> bool {
    let processes = PROCESSES.lock();
    processes.iter().any(|process| process.pid == pid)
}

/// Returns the pid of the currently scheduled process, if there is one.
pub fn current_pid() -> Option<usize> {
    let processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    processes.get(current_index).map(|process| process.pid)
}

/// The environment of the currently scheduled process.
///
/// ## Returns
/// The `KEY=VALUE` entries, or the default environment when there is no
/// current process.
pub fn current_env() -> Vec<String> {
    let processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);

    match processes.get(current_index) {
        Some(process) => process.env.clone(),
        None => DEFAULT_ENV.iter().map(|entry| String::from(*entry)).collect(),
    }
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

/// Opens a file for the current process, applying the open flags.
///
/// This is where `O_CREAT`, `O_EXCL` and `O_TRUNC` are honoured, so the
/// creation path and the plain open path are one operation rather than two
/// syscalls a caller has to sequence itself.
///
/// ## Arguments
///
/// - `path` the absolute or cwd-relative path
/// - `flags` the open flags, an access mode plus any of `O_CREAT`,
/// `O_EXCL`, `O_TRUNC` and `O_APPEND`
///
/// ## Returns
/// The new file descriptor, or the reason the open was refused.
pub fn curr_process_open_file(path: &str, flags: usize) -> Result<usize, Errno> {
    if flags & !O_SUPPORTED != 0 {
        return Err(Errno::Inval);
    }

    let (readable, writable) = match flags & O_ACCMODE {
        O_RDONLY => (true, false),
        O_WRONLY => (false, true),
        O_RDWR => (true, true),

        // the access mode is a two bit value and only three of the four are
        // defined, the fourth is not a combination of the others
        _ => return Err(Errno::Inval),
    };

    let existing = find_file_from_path(path);

    let file = match existing {
        Some(file) => {
            // O_EXCL is only meaningful with O_CREAT, and together they mean
            // the caller wants to know it was the one that created the file
            if flags & O_CREAT != 0 && flags & O_EXCL != 0 {
                return Err(Errno::Exist);
            }

            file
        }
        None => {
            if flags & O_CREAT == 0 {
                return Err(Errno::NoEnt);
            }

            let Some(file) = create_file_from_path(path) else {
                return Err(Errno::NoEnt);
            };

            file
        }
    };

    if flags & O_TRUNC != 0 {
        // truncating through a descriptor that cannot write would be a way
        // around the access mode
        if !writable {
            return Err(Errno::Inval);
        }

        if file.write().truncate(0).is_none() {
            return Err(Errno::Io);
        }
    }

    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let Some(current_process) = processes.get_mut(current_index) else {
        return Err(Errno::Srch);
    };

    Ok(current_process.open_file(file, readable, writable, flags & O_APPEND != 0))
}

/// Creates a directory for the current process.
///
/// ## Arguments
///
/// - `path` the absolute or cwd-relative path of the new directory
///
/// ## Returns
/// Whether the directory was created.
pub fn curr_process_create_directory(path: &str) -> bool {
    let Some((parent, name)) = resolve_parent_directory_and_name(path) else {
        return false;
    };

    parent.create_directory(name).is_some()
}

/// Removes a regular file for the current process.
///
/// ## Arguments
///
/// - `path` the absolute or cwd-relative path of the file to remove
///
/// ## Returns
/// Whether the file was removed.
pub fn curr_process_unlink_file(path: &str) -> bool {
    let Some((parent, name)) = resolve_parent_directory_and_name(path) else {
        return false;
    };

    parent.unlink_file(name).is_some()
}

/// Removes an empty directory for the current process.
///
/// ## Arguments
///
/// - `path` the absolute or cwd-relative path of the directory to remove
///
/// ## Returns
/// Whether the directory was removed.
pub fn curr_process_remove_directory(path: &str) -> bool {
    let Some((parent, name)) = resolve_parent_directory_and_name(path) else {
        return false;
    };

    parent.remove_directory(name).is_some()
}

fn create_file_from_path(path: &str) -> Option<Arc<RwLock<dyn File>>> {
    let (parent, name) = resolve_parent_directory_and_name(path)?;
    parent.create_file(name)
}

fn resolve_parent_directory_and_name(path: &str) -> Option<(Arc<dyn Directory>, &str)> {
    if path.is_empty() || path.ends_with('/') {
        return None;
    }

    let (base_dir, path) = if let Some(path) = path.strip_prefix("~/") {
        with_root_dir!(root, {
            let root: Arc<dyn Directory> = root;
            (root, path)
        })
    } else if let Some(path) = path.strip_prefix('/') {
        with_root_dir!(root, {
            let root: Arc<dyn Directory> = root;
            (root, path)
        })
    } else {
        (get_current_cwd(), path)
    };

    let components = normalize_path_components(path);
    let (filename, parent_components) = components.split_last()?;
    if *filename == ".." {
        return None;
    }

    let parent = if parent_components.is_empty() {
        base_dir
    } else {
        base_dir.find_directory_components(parent_components)?
    };

    Some((parent, filename))
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

/// Metadata for one of the current process' open file descriptors.
///
/// ## Arguments
///
/// - `fd` the descriptor to describe
pub fn stat_current_file_descriptor(fd: usize) -> Option<FileStat> {
    let processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get(current_index)?;

    current_process.stat_fd(fd)
}

/// Metadata for a path, which may name either a file or a directory.
///
/// ## Arguments
///
/// - `path` the absolute or cwd-relative path to describe
pub fn stat_from_path(path: &str) -> Option<FileStat> {
    // files are the common case, and a directory lookup on a file path fails
    // rather than matching something wrong, so the order only costs a miss
    if let Some(file) = find_file_from_path(path) {
        let stat = file.read().stat();
        if stat.is_some() {
            return stat;
        }
    }

    find_directory_from_path(path)?.stat()
}

/// Moves the offset of one of the current process' open file descriptors.
///
/// ## Arguments
///
/// - `fd` the descriptor to seek
/// - `offset` how far to move, relative to `whence`
/// - `whence` `SEEK_SET`, `SEEK_CUR` or `SEEK_END`
///
/// ## Returns
/// The new offset.
pub fn seek_current_file_descriptor(fd: usize, offset: isize, whence: usize) -> Option<usize> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get_mut(current_index)?;

    current_process.seek_fd(fd, offset, whence)
}

pub fn truncate_current_file_descriptor(fd: usize, size: usize) -> Option<()> {
    let mut processes = PROCESSES.lock();
    let current_index = CURRENT_INDEX.load(Ordering::SeqCst);
    let current_process = processes.get_mut(current_index)?;

    current_process.truncate_fd(fd, size)
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
