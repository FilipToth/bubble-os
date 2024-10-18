use alloc::vec::Vec;

use crate::{mem::{paging::entry::EntryFlags, PageFrame, PageFrameAllocator, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE}, print};

use super::hba::{HBACommandHeader, HBAMemory, HBAPort};

const HBA_PORT_IPM_ACTIVE: u32 = 1;
const HBA_PORT_DET_PRESENT: u32 = 3;

// SATA drive
const SATA_SIG_ATA: u32 = 0x00000101;
// SATAPI drive
const SATA_SIG_ATAPI: u32 = 0xEB140101;
// Enclosure management bridge
const SATA_SIG_SEMB: u32 = 0xC33C0101;
// Port multiplier
const SATA_SIG_PM: u32 = 0x96690101;

// Start bit
const HBA_PXCMD_ST: u32 = 0x0001;
// FIS receive enable
const HBA_PXCMD_FRE: u32 = 0x0010;
// FIS receive running
const HBA_PXCMD_FR: u32 = 0x4000;
// Command list running
const HBA_PXCMD_CR: u32 = 0x8000;

#[derive(PartialEq, Debug)]
pub enum AHCIDeviceType {
    Null,
    SATA,
    SEMB,
    PM,
    SATAPI,
}

pub struct AHCIPort {
    port_address: usize,
}

impl AHCIPort {
    fn new(port_address: usize) -> AHCIPort {
        let port = AHCIPort {
            port_address: port_address,
        };

        let frame = PageFrame::from_address(port_address);

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        controller.identity_map(frame.clone(), frame, EntryFlags::WRITABLE);

        port
    }

    fn init(&mut self) {
        self.stop_cmd();
        let port = self.get_port();

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let cl_base_frame = controller.frame_allocator.falloc().unwrap();
        let cl_base_addr = cl_base_frame.start_address();

        controller.identity_map(cl_base_frame.clone(), cl_base_frame, EntryFlags::WRITABLE);

        unsafe {
            core::ptr::write_bytes(cl_base_addr as *mut u8, 0, PAGE_SIZE);
        }

        port.clb = cl_base_addr as u32;
        port.clbu = (cl_base_addr >> 32) as u32; // this is weird

        let fis_base_frame = controller.frame_allocator.falloc().unwrap();
        let fis_base_addr = fis_base_frame.start_address();

        controller.identity_map(fis_base_frame.clone(), fis_base_frame, EntryFlags::WRITABLE);

        unsafe {
            core::ptr::write_bytes(fis_base_addr as *mut u8, 0, PAGE_SIZE);
        }

        port.fb = fis_base_addr as u32;
        port.fbu = (fis_base_addr >> 32) as u32;

        let cmd_header_addr = port.clb as usize + ((port.clbu as usize) << 32);
        let cmd_header = cmd_header_addr as *mut HBACommandHeader;

        for i in 0..32 {
            let cmd_table_base_frame = controller.frame_allocator.falloc().unwrap();
            let cmd_table_base_addr = cmd_table_base_frame.start_address();
            let base_addr = cmd_table_base_addr + (i << 8);

            controller.identity_map(cmd_table_base_frame.clone(), cmd_table_base_frame, EntryFlags::WRITABLE);


            unsafe {
                let cmd = &mut *cmd_header.add(i);
                core::ptr::write_bytes(cmd_table_base_addr as *mut u8, 0, PAGE_SIZE);

                // 8 entries per command table
                cmd.prdtl = 8;

                cmd.ctba = base_addr as u32;
                cmd.ctbau = (base_addr >> 32) as u32;
            }
        }

        self.start_cmd();
    }

    fn stop_cmd(&mut self) {
        let port = self.get_port();

        port.cmd &= !HBA_PXCMD_ST;
        port.cmd &= !HBA_PXCMD_FRE;

        loop {
            if ((port.cmd & HBA_PXCMD_FR) != 0) || ((port.cmd & HBA_PXCMD_CR) != 0) {
                continue;
            }

            break;
        }
    }

    fn start_cmd(&mut self) {
        let port = self.get_port();
        while (port.cmd & HBA_PXCMD_CR) != 0 {};

        port.cmd |= HBA_PXCMD_FRE;
        port.cmd |= HBA_PXCMD_ST;
    }

    fn read(sector: usize, length: usize) {}

    fn get_port(&mut self) -> &'static mut HBAPort {
        let port = unsafe { &mut *(self.port_address as *mut HBAPort) };
        port
    }
}

pub fn probe_ports(abar: &'static HBAMemory) -> Vec<AHCIPort> {
    let mut pi = abar.pi;
    let mut ports: Vec<AHCIPort> = Vec::new();

    // an AHCI controller can have 32 ports
    for i in 0..32 {
        if pi & 1 != 0 {
            let hba_port = &abar.ports[i];
            let port_type = check_port_type(hba_port);
            if port_type != AHCIDeviceType::SATA && port_type != AHCIDeviceType::SATAPI {
                continue;
            }

            // initialize port
            let port_address = (hba_port as *const HBAPort) as usize;
            let mut port = AHCIPort::new(port_address);

            port.init();
            ports.push(port);
        }

        pi >>= 1;
    }

    ports
}

fn check_port_type(port: &HBAPort) -> AHCIDeviceType {
    let status = port.ssts;

    // interface power management
    let ipm = (status >> 8) & 0x0F;

    // device detection
    let det = status & 0x0F;

    if det != HBA_PORT_DET_PRESENT || ipm != HBA_PORT_IPM_ACTIVE {
        return AHCIDeviceType::Null;
    }

    match port.sig {
        SATA_SIG_ATAPI => AHCIDeviceType::SATAPI,
        SATA_SIG_SEMB => AHCIDeviceType::SEMB,
        SATA_SIG_PM => AHCIDeviceType::PM,
        _ => AHCIDeviceType::SATA,
    }
}
