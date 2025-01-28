use crate::{mem::{paging::entry::EntryFlags, PageFrame, PageFrameAllocator, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE}, print};

use super::{fis::{FisRegH2D, FisType}, hba::{HBACommandHeader, HBACommandTable, HBAPort}};

// Start bit
const HBA_PXCMD_ST: u32 = 0x0001;
// FIS receive enable
const HBA_PXCMD_FRE: u32 = 0x0010;
// FIS receive running
const HBA_PXCMD_FR: u32 = 0x4000;
// Command list running
const HBA_PXCMD_CR: u32 = 0x8000;

const ATA_DEV_BUSY: u32 = 0x80;
const ATA_DEV_DRQ: u32 = 0x08;
const ATA_CMD_READ_DMA_EX: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EX: u8 = 0x35;

const HBA_PXIS_TFES: u32 = 1 << 30;

pub struct AHCIPort {
    port_address: usize,
}

impl AHCIPort {
    pub fn new(port_address: usize) -> AHCIPort {
        let mut port = AHCIPort {
            port_address: port_address,
        };

        let frame = PageFrame::from_address(port_address);

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        controller.identity_map(frame.clone(), frame, EntryFlags::WRITABLE);

        port
    }

    pub fn init(&mut self) {
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

    pub fn read(&mut self, sector: usize, sector_count: usize, buffer: *mut u8) -> bool {
        let port = self.get_port();
        port.is = u32::MAX;

        let mut spin: u64 = 0;
        while (port.tfd & (ATA_DEV_BUSY | ATA_DEV_DRQ) != 0) && spin < 1_000_000 {
            spin += 1;
        }

        if spin == 1_000_000 {
            // failed
            return false;
        }

        let cmd_header = unsafe { &mut *(port.clb as *mut HBACommandHeader) };

        let cfis_len = core::mem::size_of::<FisRegH2D>() / core::mem::size_of::<u32>();
        cmd_header.set_cfl(cfis_len as u8);
        cmd_header.set_write_bit(false);
        cmd_header.prdtl = 1;

        let cmd_table = unsafe { &mut *(cmd_header.ctba as *mut HBACommandTable) };
        let cmd_table_size = core::mem::size_of::<HBACommandTable>();

        unsafe {
            core::ptr::write_bytes(cmd_table as *mut HBACommandTable as *mut u8, 0, cmd_table_size);
        }

        let buffer_addr = buffer as usize;

        cmd_table.prdt_entry[0].data_base_address = buffer_addr as u32;
        cmd_table.prdt_entry[0].data_base_address_upper = (buffer_addr >> 32) as u32;
        cmd_table.prdt_entry[0].set_data_byte_count(((sector_count << 9) - 1) as u32);
        cmd_table.prdt_entry[0].set_interrupt_on_completion(true);

        let fis_cmd = unsafe { &mut *cmd_table.command_fis.get().cast::<FisRegH2D>() };

        fis_cmd.fis_type = FisType::RegH2D as u8;
        fis_cmd.control = 1;
        fis_cmd.command = ATA_CMD_READ_DMA_EX;

        let sector_low = sector as u32;
        let sector_high = (sector >> 32) as u32;

        fis_cmd.lba0 = sector_low as u8;
        fis_cmd.lba1 = (sector_low >> 8) as u8;
        fis_cmd.lba2 = (sector_low >> 16) as u8;
        fis_cmd.lba3 = sector_high as u8;
        fis_cmd.lba4 = (sector_high >> 8) as u8;
        fis_cmd.lba5 = (sector_high >> 16) as u8;

        // LBA mode
        fis_cmd.device = 1 << 6;

        fis_cmd.count_low = (sector_count & 0xFF) as u8;
        fis_cmd.count_high = ((sector_count >> 8) & 0xFF) as u8;

        self.get_port_ssts();

        // set command issue, dispatch command
        port.ci = 1;

        loop {
            print!("[ AHCI ] Port ci: {}\n", port.ci);
            if port.ci == 0 {
                break;
            }

            if port.is & HBA_PXIS_TFES != 0 {
                // failed
                return false;
            }
        }

        if (port.is & HBA_PXIS_TFES) != 0 {
            return false;
        }

        print!("[ AHCI ] Operation hbacmdheader bytecount transferred: {}\n", cmd_header.prdbc);

        print!("[ AHCI ] pxIS: {:b}\n", port.is);
        print!("[ AHCI ] pxTFD: {:b}\n", port.tfd);

        // success
        true
    }

    fn get_port_ssts(&mut self) {
        let port = self.get_port();
        let ssts = port.ssts;

        let det = ssts & 0x0F;
        let spd = (ssts >> 4) & 0x0F;
        let ipm = (ssts >> 8) & 0x0F;

        print!("[ AHCI ] Port det: 0x{:x}\n", det);
        print!("[ AHCI ] Port spd: 0x{:x}\n", spd);
        print!("[ AHCI ] Port ipm: 0x{:x}\n", ipm);
    }

    fn get_port(&mut self) -> &'static mut HBAPort {
        let port = unsafe { &mut *(self.port_address as *mut HBAPort) };
        port
    }
}
