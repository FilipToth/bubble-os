#[derive(Clone, Debug)]
pub struct Stack {
    pub top: usize,
    pub bottom: usize,
}

impl Stack {
    pub fn new(top: usize, bottom: usize) -> Stack {
        Stack { top, bottom }
    }
}
