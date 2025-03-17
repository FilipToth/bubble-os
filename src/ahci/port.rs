use core::alloc::Layout;

use alloc::alloc::alloc;

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
const ATA_CMD_IDENTIFY: u8 = 0xEC;

const HBA_PXIS_TFES: u32 = 1 << 30;

pub struct AHCICommand {
    buffer_addr: usize,
    data_byte_count: u32,
    cmd: u8,
    control: u8,
    lba: usize,
    count: usize,
}

pub struct AHCIPort {
    port_address: usize,
    max_slots: u32,
    block_count: u32,
}

impl AHCIPort {
    pub fn new(port_address: usize, max_slots: u32) -> AHCIPort {
        let port = AHCIPort {
            port_address: port_address,
            max_slots: max_slots,
            block_count: 0,
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
        print!("[ HBA ] Port SIG: 0x{:x}\n", port.sig);

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
        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let buffer_addr = buffer as usize;
        let buffer_addr = controller.translate_to_physical(buffer_addr).unwrap();

        print!("[ AHCI READ ] Buffer addr virtual:  0x{:X}\n", buffer as usize);
        print!("[ AHCI READ ] Buffer addr physical: 0x{:X}\n", buffer_addr);

        let command = AHCICommand {
            buffer_addr: buffer_addr,
            data_byte_count: (sector_count << 9) as u32,
            cmd: ATA_CMD_READ_DMA_EX,
            control: 1,
            lba: sector,
            count: sector_count
        };

        self.send_command(command)
    }

    pub fn send_command(&mut self, cmd: AHCICommand) -> bool {
        let port = self.get_port();
        port.is = u32::MAX;

        let mut spin: u64 = 0;
        while ((port.tfd & (ATA_DEV_BUSY | ATA_DEV_DRQ)) != 0) && spin < 1_000_000 {
            spin += 1;
        }

        print!("[ AHCI ] Spin: {}\n", spin);

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

        cmd_table.prdt_entry[0].data_base_address = cmd.buffer_addr as u32;
        cmd_table.prdt_entry[0].data_base_address_upper = (cmd.buffer_addr >> 32) as u32;
        cmd_table.prdt_entry[0].set_data_byte_count((cmd.data_byte_count - 1) as u32);
        cmd_table.prdt_entry[0].set_interrupt_on_completion(true);

        let fis_cmd = unsafe { &mut *cmd_table.command_fis.get().cast::<FisRegH2D>() };

        fis_cmd.fis_type = FisType::RegH2D as u8;
        fis_cmd.control = cmd.control;
        fis_cmd.command = cmd.cmd;

        let lba_low = cmd.lba as u32;
        let lba_high = (cmd.lba >> 32) as u32;

        fis_cmd.lba0 = lba_low as u8;
        fis_cmd.lba1 = (lba_low >> 8) as u8;
        fis_cmd.lba2 = (lba_low >> 16) as u8;
        fis_cmd.lba3 = lba_high as u8;
        fis_cmd.lba4 = (lba_high >> 8) as u8;
        fis_cmd.lba5 = (lba_high >> 16) as u8;

        // LBA mode
        fis_cmd.device = 0;

        fis_cmd.count_low = (cmd.count & 0xFF) as u8;
        fis_cmd.count_high = ((cmd.count >> 8) & 0xFF) as u8;

        // needs control bit for FIS commands
        fis_cmd.set_control_bit(true);

        self.get_port_ssts();

        let slot = match self.get_free_slot() {
            Some(s) => s,
            None => {
                // reset the port
                print!("[ AHCI ] Resetting port...\n");
                port.cmd &= !HBA_PXCMD_ST;
                port.cmd &= !HBA_PXCMD_FRE;

                // wait until reset
                while (port.cmd & (HBA_PXCMD_CR | HBA_PXCMD_FR)) != 0 {
                    core::hint::spin_loop();
                }

                port.ci = 0;
                port.sact = 0;

                port.cmd |= HBA_PXCMD_FRE;
                port.cmd |= HBA_PXCMD_ST;

                self.get_free_slot().unwrap()
            }
        };

        print!("[ AHCI ] Port CMD slot: 0x{:x}\n", slot);
        print!("[ AHCI ] Port clb: 0x{:x}\n", port.clb);
        print!("[ AHCI ] Port ctba: 0x{:x}\n", cmd_header.ctba);

        // reset byte count transferred
        cmd_header.prdbc = 0;

        // set command issue, dispatch command
        port.ci = 1 << slot;

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

        true
    }

    pub fn ahci_identify(&mut self) -> bool {
        // initialize IDENTIFY output buffer
        let layout = Layout::array::<u16>(256).unwrap();
        let buffer = unsafe { alloc(layout) };

        unsafe {
            core::ptr::write_bytes(buffer, 0, 256);
        }

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let buffer_addr = buffer as usize;
        let buffer_addr = controller.translate_to_physical(buffer_addr).unwrap();
        print!("[ AHCI BUFFER ] AHCI cmd buffer physical address: 0x{:X}\n", buffer_addr);

        // IS THE BUFFER REALLY IDENTITY MAPPED?

        let port = self.get_port();
        port.is = u32::MAX;

        let mut spin: u64 = 0;
        while ((port.tfd & (ATA_DEV_BUSY | ATA_DEV_DRQ)) != 0) && spin < 1_000_000 {
            spin += 1;
        }

        print!("[ AHCI ] Spin: {}\n", spin);

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

        cmd_table.prdt_entry[0].data_base_address = buffer_addr as u32;
        cmd_table.prdt_entry[0].data_base_address_upper = (buffer_addr >> 32) as u32;
        cmd_table.prdt_entry[0].set_data_byte_count((512 - 1) as u32);
        cmd_table.prdt_entry[0].set_interrupt_on_completion(true);

        let fis_cmd = unsafe { &mut *cmd_table.command_fis.get().cast::<FisRegH2D>() };

        fis_cmd.fis_type = FisType::RegH2D as u8;
        fis_cmd.control = 0x00;
        fis_cmd.command = ATA_CMD_IDENTIFY;

        fis_cmd.lba0 = 0;
        fis_cmd.lba1 = 0;
        fis_cmd.lba2 = 0;
        fis_cmd.lba3 = 0;
        fis_cmd.lba4 = 0;
        fis_cmd.lba5 = 0;

        // LBA mode
        fis_cmd.device = 0;

        fis_cmd.count_low = 0;
        fis_cmd.count_high = 0;

        // needs control bit for FIS commands
        fis_cmd.set_control_bit(true);

        self.get_port_ssts();

        let slot = match self.get_free_slot() {
            Some(s) => s,
            None => {
                // reset the port
                print!("[ AHCI ] Resetting port...\n");
                port.cmd &= !HBA_PXCMD_ST;
                port.cmd &= !HBA_PXCMD_FRE;

                // wait until reset
                while (port.cmd & (HBA_PXCMD_CR | HBA_PXCMD_FR)) != 0 {
                    core::hint::spin_loop();
                }

                port.ci = 0;
                port.sact = 0;

                port.cmd |= HBA_PXCMD_FRE;
                port.cmd |= HBA_PXCMD_ST;

                self.get_free_slot().unwrap()
            }
        };

        print!("[ AHCI ] Port CMD slot: 0x{:x}\n", slot);
        print!("[ AHCI ] Port clb: 0x{:x}\n", port.clb);
        print!("[ AHCI ] Port ctba: 0x{:x}\n", cmd_header.ctba);

        // reset byte count transferred
        cmd_header.prdbc = 0;

        // set command issue, dispatch command
        port.ci = 1 << slot;

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

        let buffer = unsafe { &mut *(buffer as *mut [u16; 256]) };
        let buffer = buffer.map(u16::to_be_bytes).concat();

        let block_count = u32::from_be_bytes(buffer[120..124].try_into().unwrap()).rotate_left(16);
        print!("[ AHCI ] Block count: 0x{:x}\n", block_count);

        self.block_count = block_count;

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

    fn get_free_slot(&mut self) -> Option<u32> {
        let port = self.get_port();
        let mut free_slot: Option<u32> = None;

        for i in 0..self.max_slots {
            // If neither sact nor ci has the bit set for this slot, it's free.
            if (port.sact & (1 << i)) == 0 && (port.ci & (1 << i)) == 0 {
                free_slot = Some(i);
                break;
            }
        }

        free_slot
    }
}
