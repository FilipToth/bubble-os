use core::alloc::Layout;

use alloc::alloc::{alloc, dealloc};

use crate::log;
use crate::mem::{
    paging::entry::EntryFlags, PageFrame, PageFrameAllocator, GLOBAL_MEMORY_CONTROLLER, PAGE_SIZE,
};

use super::{
    fis::{FisRegH2D, FisType},
    hba::{HBACommandHeader, HBACommandTable, HBAPort, HBA_PRDT_ENTRY_COUNT},
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
const HBA_PRDT_MAX_BYTE_COUNT: usize = 0x0040_0000;

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
        log!(crate::io::LogType::HBA, "Port SIG: 0x{:x}", port.sig);

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

            controller.identity_map(
                cmd_table_base_frame.clone(),
                cmd_table_base_frame,
                EntryFlags::WRITABLE,
            );

            unsafe {
                let cmd = &mut *cmd_header.add(i);
                core::ptr::write_bytes(cmd_table_base_addr as *mut u8, 0, PAGE_SIZE);

                cmd.prdtl = HBA_PRDT_ENTRY_COUNT as u16;

                cmd.ctba = cmd_table_base_addr as u32;
                cmd.ctbau = (cmd_table_base_addr >> 32) as u32;
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

        let command = AHCICommand {
            buffer_addr: buffer as usize,
            data_byte_count: 512,
            cmd: ATA_CMD_IDENTIFY,
            control: 0,
            lba: 0,
            count: 0,
            write: false,
            is_lba_mode: false,
        };

        if !self.send_command(command) {
            unsafe { dealloc(buffer, layout) };
            return false;
        }

        let identify_words = unsafe { &mut *(buffer as *mut [u16; 256]) };
        let identify_bytes = identify_words.map(u16::to_be_bytes).concat();

        let block_count =
            u32::from_be_bytes(identify_bytes[120..124].try_into().unwrap()).rotate_left(16);
        self.block_count = block_count;
        unsafe { dealloc(buffer, layout) };

        // success
        true
    }

    pub fn read(&mut self, sector: usize, sector_count: usize, buffer: *mut u8) -> bool {
        let command = AHCICommand {
            buffer_addr: buffer as usize,
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

    pub fn write_sector(&mut self, sector: usize, buffer: *const u8) -> bool {
        self.write_sectors(sector, 1, buffer)
    }

    pub fn write_sectors(&mut self, sector: usize, sector_count: usize, buffer: *const u8) -> bool {
        if sector_count == 0 {
            return false;
        }

        let command = AHCICommand {
            buffer_addr: buffer as usize,
            data_byte_count: (sector_count << 9) as u32,
            cmd: ATA_CMD_WRITE_DMA_EX,
            control: 1,
            lba: sector,
            count: sector_count,
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

                let Some(slot) = self.get_free_slot() else {
                    return false;
                };

                slot
            }
        };

        let cmd_header_addr = port.clb as usize + ((port.clbu as usize) << 32);
        let cmd_header =
            unsafe { &mut *((cmd_header_addr as *mut HBACommandHeader).add(slot as usize)) };

        let cfis_len = core::mem::size_of::<FisRegH2D>() / core::mem::size_of::<u32>();
        cmd_header.set_cfl(cfis_len as u8);
        cmd_header.set_write_bit(cmd.write);

        let cmd_table_addr = cmd_header.ctba as usize + ((cmd_header.ctbau as usize) << 32);
        let cmd_table = unsafe { &mut *(cmd_table_addr as *mut HBACommandTable) };
        let cmd_table_size = core::mem::size_of::<HBACommandTable>();

        unsafe {
            core::ptr::write_bytes(
                cmd_table as *mut HBACommandTable as *mut u8,
                0,
                cmd_table_size,
            );
        }

        let Some(prdt_count) =
            Self::fill_prdt_entries(cmd_table, cmd.buffer_addr, cmd.data_byte_count as usize)
        else {
            return false;
        };

        cmd_header.prdtl = prdt_count as u16;

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

    fn fill_prdt_entries(
        cmd_table: &mut HBACommandTable,
        buffer_addr: usize,
        data_byte_count: usize,
    ) -> Option<usize> {
        if data_byte_count == 0 {
            return None;
        }

        let mut controller = GLOBAL_MEMORY_CONTROLLER.lock();
        let controller = controller.as_mut()?;

        let mut virt_addr = buffer_addr;
        let mut bytes_remaining = data_byte_count;
        let mut prdt_index = 0;

        while bytes_remaining > 0 {
            if prdt_index >= HBA_PRDT_ENTRY_COUNT {
                return None;
            }

            let first_frame = controller.translate_to_physical(virt_addr)?;
            let first_page_offset = virt_addr & (PAGE_SIZE - 1);
            let buffer_phys = first_frame.start_address() + first_page_offset;
            let mut entry_size = 0;

            loop {
                let frame = controller.translate_to_physical(virt_addr)?;
                let page_offset = virt_addr & (PAGE_SIZE - 1);
                let physical_addr = frame.start_address() + page_offset;

                if entry_size > 0 && physical_addr != buffer_phys + entry_size {
                    break;
                }

                let page_bytes = PAGE_SIZE - page_offset;
                let remaining_entry_space = HBA_PRDT_MAX_BYTE_COUNT - entry_size;
                let chunk_size = core::cmp::min(
                    bytes_remaining,
                    core::cmp::min(page_bytes, remaining_entry_space),
                );

                entry_size += chunk_size;
                virt_addr += chunk_size;
                bytes_remaining -= chunk_size;

                if bytes_remaining == 0 || entry_size == HBA_PRDT_MAX_BYTE_COUNT {
                    break;
                }
            }

            let entry = &mut cmd_table.prdt_entry[prdt_index];

            entry.data_base_address = buffer_phys as u32;
            entry.data_base_address_upper = (buffer_phys >> 32) as u32;
            entry.set_data_byte_count((entry_size - 1) as u32);
            entry.set_interrupt_on_completion(bytes_remaining == 0);
            prdt_index += 1;
        }

        Some(prdt_index)
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
