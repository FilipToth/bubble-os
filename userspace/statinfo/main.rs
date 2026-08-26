#![no_std]
#![no_main]

use core::{arch::global_asm, panic::PanicInfo};

use ulib::{Args, Stat};

// runs on the kernel-provided stack, with the System V
// argument frame at the initial stack pointer
global_asm!(
    r#"
    .section .text
    .global _start

_start:
    mov rdi, [rsp]
    lea rsi, [rsp + 8]
    lea rdx, [rsi + rdi*8 + 8]
    call rust_main

    xor edi, edi
    mov rax, 1
    int 0x80

1:
    jmp 1b
"#
);

#[no_mangle]
extern "C" fn rust_main(argc: usize, argv: *const *const u8, envp: *const *const u8) -> ! {
    ulib::set_environ(envp);

    let args = Args::new(argc, argv);
    let Some(path) = args.get(1) else {
        ulib::stdout(b"Usage: statinfo <path>\n");
        ulib::exit(1);
    };

    let mut info = Stat::zero();
    if let Err(error) = ulib::stat(path, &mut info) {
        ulib::stdout(b"statinfo: ");
        ulib::stdout(path);
        ulib::stdout(b": ");
        ulib::stdout(error.as_str().as_bytes());
        ulib::stdout(b"\n");
        ulib::exit(1);
    }

    report(path, &info);

    // opening the same path and describing the descriptor has to agree with
    // the path lookup, which is the only check this program can make itself
    if info.is_file() {
        compare_with_fstat(path, &info);
    }

    ulib::exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ulib::exit(101);
}

fn report(path: &[u8], info: &Stat) {
    field(b"File", path);

    ulib::stdout(b"  Type:     ");
    ulib::stdout(type_name(info));
    ulib::stdout(b"\n");

    ulib::stdout(b"  Size:     ");
    print_number(info.size as usize);
    ulib::stdout(b" bytes\n");

    ulib::stdout(b"  Blocks:   ");
    print_number(info.blocks as usize);
    ulib::stdout(b" of ");
    print_number(info.block_size as usize);
    ulib::stdout(b" bytes\n");

    ulib::stdout(b"  Cluster:  ");
    print_number(info.inode as usize);
    ulib::stdout(b"\n");

    ulib::stdout(b"  Links:    ");
    print_number(info.links as usize);
    ulib::stdout(b"\n");

    print_time(b"  Modified: ", info.modified_time);
    print_time(b"  Accessed: ", info.accessed_time);
    print_time(b"  Created:  ", info.created_time);
}

/// Opens the path and checks that `fstat` agrees with `stat`.
///
/// The two reach the metadata by different routes, a path lookup against an
/// open descriptor, so a disagreement means one of them is wrong.
fn compare_with_fstat(path: &[u8], from_path: &Stat) {
    let Ok(fd) = ulib::open(path) else {
        return;
    };

    let mut from_fd = Stat::zero();
    let result = ulib::fstat(fd, &mut from_fd);
    let _ = ulib::close(fd);

    if result.is_err() {
        ulib::stdout(b"  (fstat on the same path failed)\n");
        return;
    }

    let agrees = from_fd.mode == from_path.mode
        && from_fd.size == from_path.size
        && from_fd.inode == from_path.inode
        && from_fd.modified_time == from_path.modified_time;

    if !agrees {
        ulib::stdout(b"  (stat and fstat disagree about this file)\n");
    }
}

fn type_name(info: &Stat) -> &'static [u8] {
    if info.is_directory() {
        b"directory"
    } else if info.is_file() {
        b"regular file"
    } else if info.is_char_device() {
        b"character device"
    } else {
        b"unknown"
    }
}

fn field(name: &[u8], value: &[u8]) {
    ulib::stdout(name);
    ulib::stdout(b":     ");
    ulib::stdout(value);
    ulib::stdout(b"\n");
}

/// Prints a Unix timestamp as `YYYY-MM-DD HH:MM:SS`.
///
/// FAT leaves a timestamp that was never written as zero, which would print
/// as 1970 and look like a real date, so it is called out instead.
fn print_time(label: &[u8], seconds: i64) {
    ulib::stdout(label);

    if seconds <= 0 {
        ulib::stdout(b"not recorded\n");
        return;
    }

    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);

    print_padded(year as usize, 4);
    ulib::stdout(b"-");
    print_padded(month as usize, 2);
    ulib::stdout(b"-");
    print_padded(day as usize, 2);
    ulib::stdout(b" ");
    print_padded((time_of_day / 3_600) as usize, 2);
    ulib::stdout(b":");
    print_padded(((time_of_day / 60) % 60) as usize, 2);
    ulib::stdout(b":");
    print_padded((time_of_day % 60) as usize, 2);
    ulib::stdout(b"\n");
}

/// Turns days since the Unix epoch back into a calendar date.
///
/// The inverse of the `days_from_civil` the kernel uses, from the same
/// Howard Hinnant algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);

    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;

    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shifted = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_shifted + 2) / 5 + 1) as u32;

    let month = if month_shifted < 10 {
        month_shifted + 3
    } else {
        month_shifted - 9
    } as u32;

    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

fn print_padded(number: usize, width: usize) {
    let mut digits = 1;
    let mut scratch = number;
    while scratch >= 10 {
        scratch /= 10;
        digits += 1;
    }

    for _ in digits..width {
        ulib::stdout(b"0");
    }

    print_number(number);
}

fn print_number(mut number: usize) {
    let mut digits = [0u8; 20];
    let mut len = 0;

    loop {
        digits[len] = b'0' + (number % 10) as u8;
        len += 1;
        number /= 10;

        if number == 0 {
            break;
        }
    }

    while len > 0 {
        len -= 1;
        ulib::stdout(&digits[len..len + 1]);
    }
}
