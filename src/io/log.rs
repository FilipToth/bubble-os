use core::fmt::{Arguments, Write};

use crate::print;

#[derive(Clone, Copy, Debug)]
pub enum LogType {
    OK,
    MEM,
    PCI,
    FS,
    ROOTFS,
    ETH,
    HBA,
    AHCI,
    SCHED,
    SYS,
    EXCEPTION,
    ERR,
    FAILED,
}

impl LogType {
    fn label(&self) -> &'static str {
        match self {
            LogType::OK => "OK",
            LogType::MEM => "MEM",
            LogType::PCI => "PCI",
            LogType::FS => "FS",
            LogType::ROOTFS => "ROOTFS",
            LogType::ETH => "ETH",
            LogType::HBA => "HBA",
            LogType::AHCI => "AHCI",
            LogType::SCHED => "SCHED",
            LogType::SYS => "SYS",
            LogType::EXCEPTION => "EXCEPTION",
            LogType::ERR => "ERR",
            LogType::FAILED => "FAILED",
        }
    }
}

pub struct Logger;

impl Write for Logger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        print!("{}", s);
        Ok(())
    }
}

pub fn log_write(log_type: LogType, args: Arguments) {
    print!("[ {} ] ", log_type.label());
    Logger.write_fmt(args).unwrap();
    print!("\n");
}

#[macro_export]
macro_rules! log {
    ($log_type:expr, $($arg:tt)*) => {
        $crate::io::log::log_write($log_type, format_args!($($arg)*))
    };
}
