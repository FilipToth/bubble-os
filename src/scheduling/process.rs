use alloc::sync::Arc;
use spin::Mutex;

use crate::{
    arch::x86_64::registers::FullInterruptStackFrame,
    elf::ElfRegion,
    fs::fs::Directory,
    mem::{paging::InactivePageTable, Stack},
};

#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,
    pub blocking: bool,
    pub awaiting_process: Option<usize>,
    pub context: FullInterruptStackFrame,
    pub start_region: Arc<Mutex<ElfRegion>>,
    pub curr_working_dir: Arc<dyn Directory + Send + Sync>,
    pub stack: Stack,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize, cwd: Arc<dyn Directory>, stack: Stack) -> Process {
        let mut context = FullInterruptStackFrame::empty();
        context.rip = entry.entry;

        Process {
            pid: pid,
            pre_schedule: true,
            blocking: false,
            awaiting_process: None,
            context: context,
            start_region: entry.start_region,
            curr_working_dir: cwd,
            stack: stack,
        }
    }
}

pub struct ProcessEntry {
    pub entry: usize,
    pub start_region: Arc<Mutex<ElfRegion>>,
    pub ring3_page_table: Option<InactivePageTable>,
}
