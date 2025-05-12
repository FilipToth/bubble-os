#[derive(Clone)]
pub struct Process {
    pub pid: usize,
    pub pre_schedule: bool,

    pub rip: usize,
    pub rsp: usize,
    pub rflags: usize,
    pub cs: usize,
    pub ss: usize,
}

impl Process {
    pub fn from(entry: ProcessEntry, pid: usize) -> Process {
        Process {
            pid: pid,
            pre_schedule: true,
            rip: entry.entry,
            rsp: 0,
            rflags: 0,
            cs: 0,
            ss: 0,
        }
    }
}

pub struct ProcessEntry {
    pub entry: usize,
}
