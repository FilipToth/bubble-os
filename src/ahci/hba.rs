use core::cell::UnsafeCell;

#[repr(C)]
pub struct HBAMemory {
    // host capability
    pub cap: u32,
    // global host control
    pub ghc: u32,
    // interrupt status
    pub is: u32,
    // port implemented
    pub pi: u32,
    // version
    pub vs: u32,
    // command completion coalescing control
    pub ccc_ctl: u32,
    // command completion coalescing ports
    pub ccc_pts: u32,
    // enclosure management location
    pub em_lock: u32,
    // enclosure management control
    pub em_ctl: u32,
    // host capabilities extended
    pub cap2: u32,
    // BIOS/OS handoff control and status
    pub bohc: u32,
    // reserved
    rsv: [u8; 0xA0 - 0x2C],
    // vendor specific registers
    pub vendor: [u8; 0x100 - 0xA0],
    // port control registers
    pub ports: [HBAPort; 32],
}

#[repr(C)]
pub struct HBAPort {
    // command list base address, 1k-byte aligned
    pub clb: u32,
    // command list base address upper 32 bits
    pub clbu: u32,
    // FIS base address, 256-byte aligned
    pub fb: u32,
    // FIS base address upper 32 bits
    pub fbu: u32,
    // interrupt status
    pub is: u32,
    // interrupt enable
    pub ie: u32,
    // command and status
    pub cmd: u32,
    // reserved
    rsv0: u32,
    // task file data
    pub tfd: u32,
    // signature
    pub sig: u32,
    // SATA status (scr0:sstatus)
    pub ssts: u32,
    // SATA control (scr2:scontrol)
    pub sctl: u32,
    // SATA error (scr1:serror)
    pub serr: u32,
    // SATA active (scr3:sactive)
    pub sact: u32,
    // command issue
    pub ci: u32,
    // SATA notification (scr4:snotification)
    pub sntf: u32,
    // FIS-based switch control
    pub fbs: u32,
    // reserved
    rsv1: [u32; 11],
    // vendor specific
    pub vendor: [u32; 4],
}

#[repr(C)]
pub struct HBACommandHeader {
    // dword 0

    // four bitfields merged into one,
    // cfl is the command FIS length in
    // dwords 2 ~ 16,
    // a is the ATAPI bit
    // w is the write bit, 1: H2D, 0: D2H
    // p is the prefetchable bit
    pub cfl_a_w_p: u8,
    // five bitfields merged into one,
    // r is the reset bit
    // b is the BIST bit
    // c is the clear busy upon R_OK bit
    // rsv0 is the first reserved bit
    // pmp is the port bultiplier, u4
    pub r_b_c_rsv0_pmp: u8,
    // physical region descriptor table
    // length in entries
    pub prdtl: u16,

    // dword 1

    // physical region descriptor table
    // byte count transferred
    pub prdbc: u16,

    // dword 2 & 3

    // command table descriptor base address
    pub ctba: u32,
    // command table descriptor base address
    // upper 32 bits
    pub ctbau: u32,

    // dword 4 - 7

    // reserved
    rsv1: [u32; 4]
}

impl HBACommandHeader {
    pub fn get_cfl(&self) -> u8 {
        self.cfl_a_w_p & 0b0001_1111
    }

    pub fn get_atapi_bit(&self) -> u8 {
        self.cfl_a_w_p & 0b0010_0000
    }

    pub fn get_write_bit(&self) -> u8 {
        self.cfl_a_w_p & 0b0100_0000
    }

    pub fn get_prefetchable(&self) -> u8 {
        self.cfl_a_w_p & 0b1000_0000
    }

    pub fn get_reset_bit(&self) -> u8 {
        self.r_b_c_rsv0_pmp & 0b0000_0001
    }

    pub fn get_bist_bit(&self) -> u8 {
        self.r_b_c_rsv0_pmp & 0b0000_0010
    }

    pub fn get_clear_busy_bit(&self) -> u8 {
        self.r_b_c_rsv0_pmp & 0b0000_0100
    }

    pub fn get_port_multiplier_port(&self) -> u8 {
        self.r_b_c_rsv0_pmp & 0b1111_0000
    }

    pub fn set_cfl(&mut self, val: u8) {
        self.cfl_a_w_p = (self.cfl_a_w_p & !0b0001_1111) | (val & 0b0001_1111);
    }

    pub fn set_atapi_bit(&mut self, val: bool) {
        if val {
            self.cfl_a_w_p |= 0b0010_0000;
        } else {
            self.cfl_a_w_p &= !0b0010_0000;
        }
    }

    pub fn set_write_bit(&mut self, val: bool) {
        if val {
            self.cfl_a_w_p |= 0b0100_0000;
        } else {
            self.cfl_a_w_p &= !0b0100_0000;
        }
    }

    pub fn set_prefetchable_bit(&mut self, val: bool) {
        if val {
            self.cfl_a_w_p |= 0b1000_0000;
        } else {
            self.cfl_a_w_p &= !0b1000_0000;
        }
    }

    pub fn set_reset_bit(&mut self, val: bool) {
        if val {
            self.r_b_c_rsv0_pmp |= 0b0000_0001;
        } else {
            self.r_b_c_rsv0_pmp &= !0b0000_0001;
        }
    }

    pub fn set_bist_bit(&mut self, val: bool) {
        if val {
            self.r_b_c_rsv0_pmp |= 0b0000_0010;
        } else {
            self.r_b_c_rsv0_pmp &= !0b0000_0010;
        }
    }

    pub fn set_clear_busy_bit(&mut self, val: bool) {
        if val {
            self.r_b_c_rsv0_pmp |= 0b0000_0100;
        } else {
            self.r_b_c_rsv0_pmp &= !0b0000_0100;
        }
    }

    pub fn set_pmport(&mut self, val: u8) {
        self.r_b_c_rsv0_pmp = (self.r_b_c_rsv0_pmp & !0b1111_0000) | (val & 0b1111_0000);
    }
}

#[repr(C)]
pub struct HBACommandTable {
    pub command_fis: UnsafeCell<[u8; 64]>,
    pub atapi_command: [u8; 16],
    rsv: [u8; 48],
    pub prdt_entry: [HBAPrdtEntry; 1]
}

#[repr(C)]
pub struct HBAPrdtEntry {
    pub data_base_address: u32,
    pub data_base_address_upper: u32,
    rsv0: u32,

    // dw3
    dbc_rsv1_i: u32
}

impl HBAPrdtEntry {
    pub fn get_data_byte_count(&self) -> u32 {
        self.dbc_rsv1_i & 0x003F_FFFF
    }

    pub fn get_interrupt_on_completion(&self) -> bool {
        (self.dbc_rsv1_i & 0x8000_0000) != 0
    }

    pub fn set_data_byte_count(&mut self, val: u32) {
        self.dbc_rsv1_i = (self.dbc_rsv1_i & !0x003F_FFFF) | (val & 0x003F_FFFF);
    }

    pub fn set_interrupt_on_completion(&mut self, val: bool) {
        if val {
            self.dbc_rsv1_i |= 0x8000_0000;
        } else {
            self.dbc_rsv1_i &= !0x8000_0000;
        }
    }
}
