use alloc::vec::Vec;

use crate::print;

use super::HBAMemory;

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

#[repr(C)]
pub struct HBAPort {
    // command list base address, 1k-byte aligned
    clb: u32,
    // command list base address upper 32 bits
    clbu: u32,
    // FIS base address, 256-byte aligned
    fb: u32,
    // FIS base address upper 32 bits
    fbu: u32,
    // interrupt status
    is: u32,
    // interrupt enable
    ie: u32,
    // command and status
    cmd: u32,
    // reserved
    rsv0: u32,
    // task file data
    tfd: u32,
    // signature
    sig: u32,
    // SATA status (scr0:sstatus)
    ssts: u32,
    // SATA control (scr2:scontrol)
    sctl: u32,
    // SATA error (scr1:serror)
    serr: u32,
    // SATA active (scr3:sactive)
    sact: u32,
    // command issue
    ci: u32,
    // SATA notification (scr4:snotification)
    sntf: u32,
    // FIS-based switch control
    fbs: u32,
    // reserved
    rsv1: [u32; 11],
    // vendor specific
    vendor: [u32; 4],
}

#[derive(PartialEq, Debug)]
pub enum AHCIDeviceType {
    Null,
    SATA,
    SEMB,
    PM,
    SATAPI,
}

pub struct AHCIPort {
    port_type: AHCIDeviceType,
    port_index: usize,
}

impl AHCIPort {
    fn new(port_type: AHCIDeviceType, port_index: usize) -> AHCIPort {
        AHCIPort {
            port_type: port_type,
            port_index: port_index,
        }
    }
}

pub fn probe_ports(abar: &HBAMemory) -> Vec<AHCIPort> {
    let mut pi = abar.pi;
    let mut ports: Vec<AHCIPort> = Vec::new();

    // an AHCI controller can have 32 ports
    for i in 0..32 {
        if pi & 1 != 0 {
            let port = &abar.ports[i];
            let port_type = check_port_type(port);

            if port_type != AHCIDeviceType::SATA && port_type != AHCIDeviceType::SATAPI {
                continue;
            }

            // initialize port
            let port = AHCIPort::new(port_type, i);
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
