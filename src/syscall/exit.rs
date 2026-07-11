// syscall 1 - exit the current process

use crate::scheduling;

pub fn exit() -> Option<usize> {
    scheduling::exit_current();
    scheduling::schedule(None);
    None
}
