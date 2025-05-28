use crate::scheduling;

pub fn exit() -> Option<usize> {
    // yield back to scheduler instead of
    // caller process
    scheduling::exit_current();
    None
}
