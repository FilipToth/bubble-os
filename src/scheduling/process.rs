use crate::{arch::x86_64::registers::FullInterruptStackFrame, mem::Region};

#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,
    pub blocking: bool,
    pub awaiting_process: Option<usize>,
    pub context: FullInterruptStackFrame,
    pub region: Region,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize) -> Process {
        let mut context = FullInterruptStackFrame::empty();
        context.rip = entry.entry;

        Process {
            pid: pid,
            pre_schedule: true,
            blocking: false,
            awaiting_process: None,
            context: context,
            region: entry.region,
        }
    }
}

// WARNING: We need to implement Send because of the raw Region pointer...
unsafe impl Send for Process {}

pub struct ProcessEntry {
    pub entry: usize,
    pub region: Region,
}
