#[derive(Debug)]
pub struct Stack {
    top: usize,
    bottom: usize,
}

impl Stack {
    pub fn new(top: usize, bottom: usize) -> Stack {
        Stack { top, bottom }
    }
}
