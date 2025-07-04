use alloc::boxed::Box;

use crate::{arch::x86_64::registers::FullInterruptStackFrame, elf::ElfRegion, fs::fs::DirectoryKind};

#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,
    pub blocking: bool,
    pub awaiting_process: Option<usize>,
    pub context: FullInterruptStackFrame,
    pub start_region: Box<ElfRegion>,
    pub curr_working_dir: DirectoryKind,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize, cwd: DirectoryKind) -> Process {
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
        }
    }
}

pub struct ProcessEntry {
    pub entry: usize,
    pub start_region: Box<ElfRegion>,
}
