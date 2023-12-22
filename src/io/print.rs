use core::fmt::{Result, Write};

pub struct EfiWriter;

impl Write for EfiWriter {
    fn write_str(&mut self, s: &str) -> Result {
        crate::io::serial::serial_write_str(s);
        Ok(())
    }
}

/// macro for printing to efi console output
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (
        <$crate::io::print::EfiWriter as core::fmt::Write>::write_fmt(
            &mut $crate::io::print::EfiWriter,
            format_args!($($arg )*)
        ).unwrap();
    );
}
