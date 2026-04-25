use core::{
    ops::Add,
    ptr::{read_volatile, write_volatile},
};

use x86_64::{structures::idt::InterruptStackFrame};

use crate::{
    arch::{
        self,
        x86_64::{acpi::pci::{BarType, PciDevice, PciDeviceHeaderType0}, idt::IRQ0, pit::end_of_interrupt},
    },
    mem::{
        GLOBAL_MEMORY_CONTROLLER, PageFrame, paging::{Page, entry::EntryFlags}
    },
    net::{ETH_DRIVER, dma_ptr::DMAPtr},
    print,
};

// https://pdos.csail.mit.edu/6.828/2019/readings/hardware/8254x_GBe_SDM.pdf

const DMA_REGION_START: usize = 0x0000_6DCF_0000_0000;

const REG_CONTROL: usize = 0x0000;
const REG_STATUS: usize = 0x0008;
const REG_EEPROM: usize = 0x0014;
const REG_ICR: usize = 0x00C0; // Interrupt Control Register
const REG_MAC: usize = 0x5400;
const REG_MTA: usize = 0x5200; // Multicast Table
const REG_IMASK: usize = 0x00D0;

const REG_RCTRL: usize = 0x0100;
const REG_RXDESCLO: usize = 0x2800;
const REG_RXDESCHI: usize = 0x2804;
const REG_RXDESCLEN: usize = 0x2808;
const REG_RXDESCHEAD: usize = 0x2810;
const REG_RXDESCTAIL: usize = 0x2818;

const REG_TXDESCLO: usize = 0x3800;
const REG_TXDESCHI: usize = 0x3804;

const REG_TPR: usize = 0x40D0;
const REG_MPC: usize = 0x4010;
const REG_PRC64: usize = 0x405C;
const REG_PRC127: usize = 0x4060;

const CTRL_LRST: u32 = 1 << 3; // Reset Link
const CTRL_ASDE: u32 = 1 << 5; // Enable Auto-Speed Detection
const CTRL_SLU: u32 = 1 << 6; // Set Link Up
const CTRL_RST: u32 = 1 << 26; // Reset

const RCTL_EN: usize = 1 << 1; // Receiver Enable
const RCTL_SBP: usize = 1 << 2; // Store Bad Packets
const RCTL_UPE: usize = 1 << 3; // Unicast Promiscuous Enabled
const RCTL_MPE: usize = 1 << 4; // Multicast Promiscuous Enabled
const RCTL_LPE: usize = 1 << 5; // Long Packet Reception Enable
const RCTL_LBM_NONE: usize = 0 << 6; // No Loopback
const RCTL_LBM_PHY: usize = 3 << 6; // PHY or external SerDesc loopback
const RTCL_RDMTS_HALF: usize = 0 << 8; // Free Buffer Threshold is 1/2 of RDLEN
const RTCL_RDMTS_QUARTER: usize = 1 << 8; // Free Buffer Threshold is 1/4 of RDLEN
const RTCL_RDMTS_EIGHTH: usize = 2 << 8; // Free Buffer Threshold is 1/8 of RDLEN
const RCTL_MO_36: usize = 0 << 12; // Multicast Offset - bits 47:36
const RCTL_MO_35: usize = 1 << 12; // Multicast Offset - bits 46:35
const RCTL_MO_34: usize = 2 << 12; // Multicast Offset - bits 45:34
const RCTL_MO_32: usize = 3 << 12; // Multicast Offset - bits 43:32
const RCTL_BAM: usize = 1 << 15; // Broadcast Accept Mode
const RCTL_VFE: usize = 1 << 18; // VLAN Filter Enable
const RCTL_CFIEN: usize = 1 << 19; // Canonical Form Indicator Enable
const RCTL_CFI: usize = 1 << 20; // Canonical Form Indicator Bit Value
const RCTL_DPF: usize = 1 << 22; // Discard Pause Frames
const RCTL_PMCF: usize = 1 << 23; // Pass MAC Control Frames
const RCTL_SECRC: usize = 1 << 26; // Strip Ethernet CRC

const RCTL_BUFFER_SIZE_4096: usize = (3 << 16) | (1 << 25);
const RCTL_BUFFER_SIZE_8192: usize = (2 << 16) | (1 << 25);
const RCTL_BUFFER_SIZE_16384: usize = (1 << 16) | (1 << 25);

const ICR_RXT0: u32 = 1 << 7; // RX Timer Interrupt
const ICR_RXDMT0: u32 = 1 << 4; // RX Descriptor Threshold
const ICR_RXSEQ: u32 = 1 << 3; // RX Sequence Error
const ICR_LSC: u32 = 1 << 2; // Link Status Change

const NUM_RX_BUFFERS: usize = 32;
const NUM_TX_BUFFERS: usize = 8;
const BUFFER_SIZE: usize = 0x1000;

#[repr(C, align(16))]
struct RxDesc {
    addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C, align(16))]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

pub struct E1000Driver {
    bar_type: BarType,
    eeprom_exists: bool,
    interrupt_line: u8,
    rx_curr: usize,
    rx_descs: DMAPtr<RxDesc>,
    mac: [u8; 6],
}

impl E1000Driver {
    pub fn new(eth: &PciDevice) -> E1000Driver {
        let addr = eth.pci_base_addr;
        let header = unsafe { &mut *(addr as *mut PciDeviceHeaderType0) };

        let bar0_type = header.get_bar_type(0).unwrap();
        print!("[ ETH ] vendor id: 0x{:X}\n", header.subsystem_vendor_id);
        print!("[ ETH ] bar0: {:#?}\n", bar0_type);

        let bar_size = unsafe { header.probe_bar_size(0) }.unwrap();
        print!("[ ETH ] bar0 size: 0x{:X}\n", bar_size);

        let addr = match bar0_type {
            BarType::IO { address } => address as usize,
            BarType::Memory32 { address, .. } => address as usize,
            BarType::Memory64 { address, .. } => address as usize,
        };

        let start = PageFrame::from_address(addr);
        let end = PageFrame::from_address(addr + bar_size as usize);

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        controller.identity_map(start, end, EntryFlags::WRITABLE);

        header.enable_bus_mastering();

        E1000Driver {
            bar_type: bar0_type,
            eeprom_exists: false,
            interrupt_line: header.interrupt_line,
            rx_curr: 0,
            rx_descs: DMAPtr::null(),
            mac: [0; 6],
        }
    }

    pub fn start(&mut self) {
        let eeprom_exists = self.detect_eeprom();
        self.eeprom_exists = eeprom_exists;
        print!("[ ETH ] Is EEPROM: {}\n", eeprom_exists);

        let mac = self.read_mac_address().unwrap();
        self.print_mac(mac);
        self.mac = mac;

        self.link_up();

        // clear multicast table
        for i in 0..80 {
            let offset = REG_MTA + (i * 4);
            self.send_command(offset, 0);
        }

        // register interrupt
        let vector = arch::x86_64::idt::IRQ0 + (self.interrupt_line as usize);
        let handler = handle_interrupt as usize;

        unsafe {
            arch::x86_64::idt::register_interrupt(vector, handler, false);
        }

        self.write_mac_into_address_reg();

        self.start_rx();

        self.enable_interrupt();

        let rctl = self.read_command(REG_RCTRL);
        print!("[ ETH ] RCTL: 0x{:X}\n", rctl);

        print!("[ ETH ] Up!\n");
    }

    #[allow(dead_code)]
    pub fn poll(&self, print_all: bool) -> bool {
        if self.rx_descs.is_null() {
            print!("[ ETH ] Rx Descriptor Table Pointer is null\n");
            return false;
        }

        let mut ptr = unsafe { self.rx_descs.as_ptr() };
        for i in 0..NUM_RX_BUFFERS {
            let status = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*ptr).status)) };
            let length = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*ptr).length)) };
            let errors = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*ptr).errors)) };

            if status & 0x1 != 0 || print_all {
                print!(
                    "[ ETH ] RX Buffer ({}), status: 0x{:X}, len: 0x{:X}, errors: 0x{:X}\n",
                    i, status, length, errors
                );

                if !print_all {
                    return true;
                }
            }

            ptr = unsafe { ptr.add(1) };
        }

        if print_all {
            // print chip diagnostics
            let status = self.read_command(REG_STATUS);
            let trp = self.read_command(REG_TPR);
            let mpc = self.read_command(REG_MPC);

            let prc64 = self.read_command(REG_PRC64);
            let prc127 = self.read_command(REG_PRC127);

            print!(
                "[ ETH ] STATUS: 0x{:X}, TRP: 0x{:X}, MPC: 0x{:X}, PRC64: 0x{:X}, PRC127: 0x{:X}\n",
                status, trp, mpc, prc64, prc127
            );
        }

        return false;
    }

    fn start_rx(&mut self) -> bool {
        print!("[ ETH ] Starting ETH RX\n");
        let mut mc = GLOBAL_MEMORY_CONTROLLER.lock();
        let mc = mc.as_mut().unwrap();

        // allocate descriptor table
        let descs_size = size_of::<RxDesc>() * NUM_RX_BUFFERS;
        let descs_start_page = Page::for_address(DMA_REGION_START);
        let descs_end_page = Page::for_address(DMA_REGION_START + descs_size - 1);

        mc.map(descs_start_page, descs_end_page, EntryFlags::WRITABLE);
        let descs_ptr = DMA_REGION_START as *mut RxDesc;

        unsafe {
            core::ptr::write_bytes(descs_ptr as *mut u8, 0, descs_size);
        }

        // allocate buffers
        let mut ptr = descs_ptr;
        let mut last_page = descs_end_page.add(1);

        for _ in 0..NUM_RX_BUFFERS {
            let start_page = last_page;
            let end_page = Page::for_address(start_page.start_address() + BUFFER_SIZE - 1);

            mc.map(start_page, end_page, EntryFlags::WRITABLE);
            last_page = end_page.add(1);

            let buffer_pa = {
                // get physical addr for buffer
                let buffer_va = start_page.start_address();
                let frame = mc
                    .kernel_table
                    .translate_to_phys(buffer_va, &mut mc.temp_mapper)
                    .unwrap();

                let offset = buffer_va & 0xFFF;
                frame.start_address() + offset
            };

            let desc = unsafe { &mut *ptr };
            desc.addr = buffer_pa as u64;

            desc.length = 0;
            desc.checksum = 9;
            desc.status = 0;
            desc.errors = 0;
            desc.special = 0;

            ptr = unsafe { ptr.add(1) };
        }

        let descsc_pa = {
            let va = descs_ptr as usize;
            let frame = mc
                .kernel_table
                .translate_to_phys(va, &mut mc.temp_mapper)
                .unwrap();

            let offset = va & 0xFFF;
            frame.start_address() + offset
        };

        self.send_command(REG_RXDESCLO, (descsc_pa & 0xFFFF_FFFF) as u32);
        self.send_command(REG_RXDESCHI, (descsc_pa >> 32) as u32);

        self.send_command(REG_RXDESCLEN, descs_size as u32);

        self.send_command(REG_RXDESCHEAD, 0);
        self.send_command(REG_RXDESCTAIL, (NUM_RX_BUFFERS - 1) as u32);

        self.rx_curr = 0;
        self.rx_descs = DMAPtr::from_ptr(ptr);

        self.send_command(
            REG_RCTRL,
            (RCTL_EN
                | RCTL_SBP
                | RCTL_UPE
                | RCTL_MPE
                | RCTL_LBM_NONE
                | RTCL_RDMTS_HALF
                | RCTL_BAM
                | RCTL_SECRC
                | RCTL_BUFFER_SIZE_4096) as u32,
        );

        return true;
    }

    fn write_mac_into_address_reg(&self) {
        let mac = self.mac;

        // RAL
        let ral = (mac[0] as u32)
            | ((mac[1] as u32) << 8)
            | ((mac[2] as u32) << 16)
            | ((mac[3] as u32) << 24);

        // RAH
        let rah = (mac[4] as u32) | ((mac[5] as u32) << 8) | (1 << 31); // VALID bit

        self.send_command(REG_MAC, ral);
        self.send_command(REG_MAC + 0x04, rah);
    }

    fn enable_interrupt(&self) {
        self.send_command(REG_IMASK, 0x1F6DC);
        self.send_command(REG_IMASK, 0xFF & !4);
        self.read_command(0xC0);
    }

    fn link_up(&self) {
        let ctl = self.read_command(REG_CONTROL);
        let new_ctl = (ctl | CTRL_SLU | CTRL_ASDE) & !CTRL_LRST;
        self.send_command(REG_CONTROL, new_ctl);
    }

    fn detect_eeprom(&self) -> bool {
        self.send_command(REG_EEPROM, 0x1);

        for _ in 0..1000 {
            let val = self.read_command(REG_EEPROM);
            if val & 0x10 != 0 {
                return true;
            }
        }

        return false;
    }

    fn read_mac_address(&self) -> Option<[u8; 6]> {
        // TODO: Implement EEPROM
        if self.eeprom_exists && false {
            unimplemented!()
        } else {
            let base_addr = match self.bar_type {
                BarType::IO { .. } => unimplemented!(),
                BarType::Memory32 { address, .. } => address,
                BarType::Memory64 { .. } => unimplemented!(),
            };

            let mac_base = (base_addr as usize) + REG_MAC;
            let mut base_mac_8 = mac_base as *const u8;
            let base_mac_32 = mac_base as *const u32;

            if unsafe { *base_mac_32 } == 0 {
                return None;
            }

            let mut mac: [u8; 6] = [0; 6];
            for i in 0..6 {
                mac[i] = unsafe { *base_mac_8 };
                base_mac_8 = unsafe { base_mac_8.add(1) };
            }

            Some(mac)
        }
    }

    fn print_mac(&self, mac: [u8; 6]) {
        print!("[ ETH ] Mac Address: ");

        for i in 0..mac.len() {
            let m = mac[i];
            print!("{:X}", m);

            if i != mac.len() - 1 {
                print!(":")
            }
        }

        print!("\n");
    }

    fn send_command(&self, p_address: usize, p_value: u32) {
        match self.bar_type {
            BarType::IO { .. } => unimplemented!(),
            BarType::Memory32 { address, .. } => {
                let addr = (address as usize) + p_address;
                let ptr = addr as *mut u32;
                unsafe { write_volatile(ptr, p_value) };
            }
            BarType::Memory64 { .. } => unimplemented!(),
        };
    }

    fn read_command(&self, p_address: usize) -> u32 {
        match self.bar_type {
            BarType::IO { .. } => unimplemented!(),
            BarType::Memory32 { address, .. } => {
                let addr = (address as usize) + p_address;
                let ptr = addr as *mut u32;
                unsafe { read_volatile(ptr) }
            }
            BarType::Memory64 { .. } => unimplemented!(),
        }
    }

    fn handle_rx(&self) {
        let icr = self.read_command(REG_ICR);
        
        print!("[ ETH ] Received RX, icr: 0x{:X}\n", icr);

        self.poll(true);
    }
}

extern "x86-interrupt" fn handle_interrupt(_stack: InterruptStackFrame) {
    let mut guard = ETH_DRIVER.lock();

    if let Some(driver) = guard.as_mut() {
        driver.handle_rx();

        // send EOI
        let vector = IRQ0 + driver.interrupt_line as usize;
        end_of_interrupt(vector as u8);
    }
}
