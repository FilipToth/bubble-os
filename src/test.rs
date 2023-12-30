use crate::print;

pub struct TestUnit<'a> {
    function: &'a dyn Fn() -> bool,
    succeeded: bool,
    name: &'a str
}

impl<'a> TestUnit<'a> {
    pub fn new(func: &'a dyn Fn() -> bool, name: &'a str) -> TestUnit<'a> {
        let mut unit = TestUnit {
            function: func,
            succeeded: false,
            name: name
        };

        unit.run();
        unit
    }

    pub fn run(&mut self) {
        let success = (self.function)();
        self.succeeded = success;

        if success {
            print!("[   OK   ] Case {} succeeded!\n", self.name);
        } else {
            print!("[ FAILED ] Case {} failed!\n", self.name);
        }
    }
}

#[macro_export]
macro_rules! assert_true {
    ($($x:expr)*) => {
        if !( $($x)* ) {
            crate::print!("Test case failed...");
            return false;
        }
    };
}

pub fn run_tests(boot_info_addr: usize) {
    TestUnit::new(&test, "Initialization Tests");
}

pub fn test() -> bool {
    assert_true!(true);
    return true;
}
