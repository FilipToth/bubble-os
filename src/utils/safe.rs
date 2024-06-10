pub struct Safe<A> {
    mutex: spin::Mutex<A>
}

impl<A> Safe<A> {
    pub const fn new(obj: A) -> Safe<A> {
        Safe { mutex: spin::Mutex::new(obj) }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.mutex.lock()
    }
}
