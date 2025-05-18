use crate::arch::x86_64::timer_isr::FullInterruptStackFrame;

#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,
    pub context: FullInterruptStackFrame,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize) -> Process {
        let mut context = FullInterruptStackFrame::empty();
        context.rip = entry.entry;

        Process {
            pid: pid,
            pre_schedule: true,
            context: context,
        }
    }
}

pub struct ProcessEntry {
    pub entry: usize,
}
