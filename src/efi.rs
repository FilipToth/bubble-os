use core::sync::atomic::{AtomicPtr, Ordering};

static EFI_SYSTEM_TABLE: AtomicPtr<EfiSystemTable> = AtomicPtr::new(core::ptr::null_mut());

pub unsafe fn register_efi_system_table(system_table: *mut EfiSystemTable) {
    EFI_SYSTEM_TABLE
        .compare_exchange(
            core::ptr::null_mut(),
            system_table,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .unwrap();
}

pub fn output_text(text: &str) {
    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);
    if system_table.is_null() {
        return;
    }

    let console_out = unsafe { (*system_table).console_out };

    // convert utf8 to ucs-2
    // use a 32 byte buffer
    let mut tmp = [0u16; 32];
    let mut in_use = 0;

    for chr in text.encode_utf16() {
        // need to have carriage returns
        // for serial printing and other devices
        if chr == b'\n' as u16 {
            tmp[in_use] = b'\r' as u16;
            in_use += 1;
        }

        tmp[in_use] = chr;
        in_use += 1;

        // write out the buffer if it's full
        if in_use == (tmp.len() - 2) {
            tmp[in_use] = 0;

            unsafe {
                ((*console_out).output_string)(console_out, tmp.as_ptr());
            }

            in_use = 0;
        }
    }

    // write any leftover characters
    if in_use > 0 {
        tmp[in_use] = 0;
        unsafe {
            ((*console_out).output_string)(console_out, tmp.as_ptr());
        }
    }
}

/// Gets the starting address of the acpi table
pub fn get_acpi_table() -> Option<usize> {
    // acpi 1.0
    let acpi_table_guid = EfiGuid(
        0xeb9d2d30,
        0x2d88,
        0x11d3,
        [0x9a, 0x16, 0x0, 0x90, 0x27, 0x3f, 0xc1, 0x4d],
    );

    // acpi 2.0+
    let efi_acpi_table_guid = EfiGuid(
        0x8868e871,
        0xe4f1,
        0x11d3,
        [0xbc, 0x22, 0x0, 0x80, 0xc7, 0x3c, 0x88, 0x81],
    );

    let system_table = EFI_SYSTEM_TABLE.load(Ordering::SeqCst);
    if system_table.is_null() {
        return None;
    }

    let tables = unsafe {
        core::slice::from_raw_parts((*system_table).tables, (*system_table).number_of_tables)
    };

    let acpi_table = tables
        .iter()
        .find_map(|EfiConfigurationTable { guid, table }| {
            if *guid == efi_acpi_table_guid {
                Some(*table)
            } else {
                None
            }
        })
        .or_else(|| {
            tables
                .iter()
                .find_map(|EfiConfigurationTable { guid, table }| {
                    if *guid == acpi_table_guid {
                        Some(*table)
                    } else {
                        None
                    }
                })
        });

    acpi_table
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct EfiMemoryDescriptor {
    // type of memory region
    typ: u32,
    physical_start: u64,
    virtual_start: u64,
    number_of_pages: u64,
    attribute: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
enum EfiMemoryType {
    ReservedMemoryType,
    LoaderCode,
    LoaderData,
    BootServicesCode,
    BootServicesData,
    RuntimeServicesCode,
    RuntimeServicesData,
    ConventionalMemory,
    UnusableMemory,
    ACPIReclaimMemory,
    ACPIMemoryNvs,
    MemoryMappedIO,
    MemoryMappedIOPortSpace,
    PalCode,
    PersistentMemory,
    Invalid,
}

impl From<u32> for EfiMemoryType {
    fn from(mem_type_code: u32) -> EfiMemoryType {
        match mem_type_code {
            0 => EfiMemoryType::ReservedMemoryType,
            1 => EfiMemoryType::LoaderCode,
            2 => EfiMemoryType::LoaderData,
            3 => EfiMemoryType::BootServicesCode,
            4 => EfiMemoryType::BootServicesData,
            5 => EfiMemoryType::RuntimeServicesCode,
            6 => EfiMemoryType::RuntimeServicesData,
            7 => EfiMemoryType::ConventionalMemory,
            8 => EfiMemoryType::UnusableMemory,
            9 => EfiMemoryType::ACPIReclaimMemory,
            10 => EfiMemoryType::ACPIMemoryNvs,
            11 => EfiMemoryType::MemoryMappedIO,
            12 => EfiMemoryType::MemoryMappedIOPortSpace,
            13 => EfiMemoryType::PalCode,
            14 => EfiMemoryType::PersistentMemory,
            _ => EfiMemoryType::Invalid,
        }
    }
}

impl EfiMemoryType {
    fn is_available_after_boot_services_exit(&self) -> bool {
        match self {
            EfiMemoryType::BootServicesCode
            | EfiMemoryType::BootServicesData
            | EfiMemoryType::ConventionalMemory
            | EfiMemoryType::PersistentMemory => true,
            _ => false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiHandle(usize);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiStatus(pub usize);

#[repr(C)]
pub struct EfiSystemTable {
    header: EfiTableHeader,
    firmware_vendor: *const u16,
    firmware_revision: u32,
    console_in_handle: EfiHandle,
    console_in: *const EfiSimpleTextInputProtocol,
    console_out_handle: EfiHandle,
    console_out: *const EfiSimpleTextOutputProtocol,
    console_err_handle: EfiHandle,
    console_err: *const EfiSimpleTextOutputProtocol,
    _runtime_services: usize,
    boot_services: *const EfiBootServices,
    number_of_tables: usize,
    tables: *const EfiConfigurationTable,
}

#[repr(C)]
struct EfiSimpleTextInputProtocol {
    reset: unsafe fn(
        this: *const EfiSimpleTextInputProtocol,
        extended_verification: bool,
    ) -> EfiStatus,
    read_keystroke:
        unsafe fn(this: *const EfiSimpleTextInputProtocol, key: *mut EfiInputKey) -> EfiStatus,
    _wait_for_key: usize,
}

#[repr(C)]
struct EfiInputKey {
    scan_code: u16,
    unicode_char: u16,
}

#[repr(C)]
struct EfiSimpleTextOutputProtocol {
    reset: unsafe fn(
        this: *const EfiSimpleTextOutputProtocol,
        extended_verification: bool,
    ) -> EfiStatus,
    output_string:
        unsafe fn(this: *const EfiSimpleTextOutputProtocol, string: *const u16) -> EfiStatus,
    test_string:
        unsafe fn(this: *const EfiSimpleTextOutputProtocol, string: *const u16) -> EfiStatus,
    _query_mode: usize,
    _set_mode: usize,
    _set_attribute: usize,
    _clear_screen: usize,
    _set_cursor_position: usize,
    _enable_cursor: usize,
    _mode: usize,
}

#[repr(C)]
struct EfiBootServices {
    header: EfiTableHeader,
    _raise_tpl: usize,
    _restore_tpl: usize,
    _allocate_pages: usize,
    _free_pages: usize,

    get_memory_map: unsafe fn(
        memory_map_size: &mut usize,
        memory_map: *mut u8,
        map_key: &mut usize,
        descriptor_size: &mut usize,
        descriptor_version: &mut u32,
    ) -> EfiStatus,
    _allocate_pool: usize,
    _free_pool: usize,
    _create_event: usize,
    _set_timer: usize,
    _wait_for_event: usize,
    _signal_event: usize,
    _close_event: usize,
    _check_event: usize,
    _install_protocol_interface: usize,
    _reinstall_protocol_interface: usize,
    _uninstall_protocol_interface: usize,
    _handle_protocol: usize,
    _reserved: usize,
    _register_protocol_notify: usize,
    _locate_handle: usize,
    _locate_device_path: usize,
    _install_configuration_table: usize,
    _load_image: usize,
    _start_image: usize,
    _exit: usize,
    _unload_image: usize,
    exit_boot_services: unsafe fn(image_handle: EfiHandle, map_key: usize) -> EfiStatus,
}

#[repr(C)]
struct EfiTableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
struct EfiConfigurationTable {
    guid: EfiGuid,
    table: usize,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
struct EfiGuid(u32, u16, u16, [u8; 8]);
