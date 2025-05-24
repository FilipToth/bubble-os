use crate::arch::x86_64::registers::FullInterruptStackFrame;

#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,
    pub blocking: bool,
    pub context: FullInterruptStackFrame,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize) -> Process {
        let mut context = FullInterruptStackFrame::empty();
        context.rip = entry.entry;

        Process {
            pid: pid,
            pre_schedule: true,
            blocking: false,
            context: context,
        }
    }
}

pub struct ProcessEntry {
    pub entry: usize,
}
