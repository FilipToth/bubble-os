use core::alloc::Layout;

use alloc::{alloc::alloc, string::String};

use crate::{
    mem::{
        paging::entry::EntryFlags, PageFrame, PageFrameAllocator, GLOBAL_MEMORY_CONTROLLER,
        PAGE_SIZE,
    },
    print,
};

use super::{
    fis::{FisRegH2D, FisType},
    hba::{HBACommandHeader, HBACommandTable, HBAPort},
};

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
    write: bool,
    is_lba_mode: bool,
}

pub struct AHCIPort {
    port_address: usize,
    max_slots: u32,
    block_count: u32,
}

struct PortStatus {
    det: u32,
    spd: u32,
    ipm: u32,
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

            controller.identity_map(
                cmd_table_base_frame.clone(),
                cmd_table_base_frame,
                EntryFlags::WRITABLE,
            );

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
        while (port.cmd & HBA_PXCMD_CR) != 0 {}

        port.cmd |= HBA_PXCMD_FRE;
        port.cmd |= HBA_PXCMD_ST;
    }

    pub fn ahci_identify(&mut self) -> bool {
        // initialize IDENTIFY output buffer
        let layout = Layout::array::<u16>(256).unwrap();
        let buffer = unsafe { alloc(layout) };

        unsafe {
            core::ptr::write_bytes(buffer, 0, 512);
        }

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let buffer_addr = buffer as usize;
        let buffer_addr = controller.translate_to_physical(buffer_addr).unwrap();

        let command = AHCICommand {
            buffer_addr: buffer_addr,
            data_byte_count: 512,
            cmd: ATA_CMD_IDENTIFY,
            control: 0,
            lba: 0,
            count: 0,
            write: false,
            is_lba_mode: false,
        };

        if !self.send_command(command) {
            return false;
        }

        let buffer = unsafe { &mut *(buffer as *mut [u16; 256]) };
        let buffer = buffer.map(u16::to_be_bytes).concat();

        let block_count = u32::from_be_bytes(buffer[120..124].try_into().unwrap()).rotate_left(16);
        print!("[ AHCI ] Block count: 0x{:x}\n", block_count);

        self.block_count = block_count;

        // success
        true
    }

    pub fn read(&mut self, sector: usize, sector_count: usize, buffer: *mut u8) -> bool {
        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let buffer_addr = buffer as usize;
        let buffer_addr = controller.translate_to_physical(buffer_addr).unwrap();

        let command = AHCICommand {
            buffer_addr: buffer_addr,
            data_byte_count: (sector_count << 9) as u32,
            cmd: ATA_CMD_READ_DMA_EX,
            control: 1,
            lba: sector,
            count: sector_count,
            write: false,
            is_lba_mode: true,
        };

        self.send_command(command)
    }

    pub fn write(&mut self, sector: usize, content: &str) -> bool {
        let length = content.len();
        let length = core::cmp::max(length, 511);

        let layout = Layout::array::<u8>(length).unwrap();
        let buffer = unsafe { alloc(layout) };

        let bytes = content.as_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, content.len());
        }

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut().unwrap();

        let buffer_addr = buffer as usize;
        let buffer_addr = controller.translate_to_physical(buffer_addr).unwrap();

        let command = AHCICommand {
            buffer_addr: buffer_addr,
            data_byte_count: 512,
            cmd: ATA_CMD_WRITE_DMA_EX,
            control: 1,
            lba: sector,
            count: 1,
            write: true,
            is_lba_mode: true,
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

        if spin == 1_000_000 {
            // failed
            return false;
        }

        let cmd_header = unsafe { &mut *(port.clb as *mut HBACommandHeader) };

        let cfis_len = core::mem::size_of::<FisRegH2D>() / core::mem::size_of::<u32>();
        cmd_header.set_cfl(cfis_len as u8);
        cmd_header.set_write_bit(cmd.write);
        cmd_header.prdtl = 1;

        let cmd_table = unsafe { &mut *(cmd_header.ctba as *mut HBACommandTable) };
        let cmd_table_size = core::mem::size_of::<HBACommandTable>();

        unsafe {
            core::ptr::write_bytes(
                cmd_table as *mut HBACommandTable as *mut u8,
                0,
                cmd_table_size,
            );
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

        fis_cmd.device = if cmd.is_lba_mode { 1 << 6 } else { 0 };

        fis_cmd.count_low = (cmd.count & 0xFF) as u8;
        fis_cmd.count_high = ((cmd.count >> 8) & 0xFF) as u8;

        // needs control bit for FIS commands
        fis_cmd.set_control_bit(true);

        let slot = match self.get_free_slot() {
            Some(s) => s,
            None => {
                // reset the port
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

        // reset byte count transferred
        cmd_header.prdbc = 0;

        // set command issue, dispatch command
        port.ci = 1 << slot;

        loop {
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

        true
    }

    fn get_port_ssts(&mut self) -> PortStatus {
        let port = self.get_port();
        let ssts = port.ssts;

        let det = ssts & 0x0F;
        let spd = (ssts >> 4) & 0x0F;
        let ipm = (ssts >> 8) & 0x0F;

        PortStatus {
            det: det,
            spd: spd,
            ipm: ipm,
        }
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
