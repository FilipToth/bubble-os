use core::fmt::{Result, Write};

pub struct EfiWriter;

impl Write for EfiWriter {
    fn write_str(&mut self, s: &str) -> Result {
        crate::efi::output_text(s);
        Ok(())
    }
}

/// macro for printing to efi console output
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (
        <$crate::print::EfiWriter as core::fmt::Write>::write_fmt(
            &mut $crate::print::EfiWriter,
            format_args!($($arg )*)
        ).unwrap();
    );
}
