use crate::arch::x86_64::acpi::pci::PciDevice;

enum FisType {
    RegH2D = 0x27,
    RegD2H = 0x34,
    DMAAct = 0x39,
    DMASetup = 0x41,
    Data = 0x46,
    BIST = 0x58,
    PIOSetup = 0x5F,
    DevBits = 0xA1,
}

#[repr(C)]
struct FisRegH2D {
    // dword 0

    // should be 0x27
    fis_type: u8,
    // 3 bitfields merged into one u8,
    // pmport is the port multiplier
    // rsv0 are the three reserved bits
    // and c is the 1: command, 0: control
    pmport_rsv0_c: u8,
    // command register
    command: u8,
    // feature register 7:0
    feature_low: u8,

    // dword 1

    // lba low register, 7:0
    lba0: u8,
    // lba mid register, 15:8
    lba1: u8,
    // lba high register, 23:16
    lba2: u8,
    // device register
    device: u8,

    // dword 2

    // lba register, 31:24
    lba3: u8,
    // lba register, 39:32
    lba4: u8,
    // lba register, 47:40
    lba5: u8,
    // feature register 15:8
    feature_high: u8,

    // dword 3

    // count register 7:0
    count_low: u8,
    // count register 15:8
    count_high: u8,
    // isochronous command completion
    icc: u8,
    // control register
    control: u8,

    // dword 4

    // reserved
    rsv1: [u8; 4],
}

impl FisRegH2D {
    fn get_pmport(&self) -> u8 {
        self.pmport_rsv0_c & 0b0000_1111
    }

    fn get_control_bit(&self) -> u8 {
        self.pmport_rsv0_c & 0b1000_0000
    }

    fn set_pmport(&mut self, val: u8) {
        self.pmport_rsv0_c = (self.pmport_rsv0_c & 0b1111_0000) | (val & 0b0000_1111);
    }

    fn set_control_bit(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_c |= 0b1000_0000;
        } else {
            self.pmport_rsv0_c &= !0b1000_0000;
        }
    }
}

#[repr(C)]
struct FisRegD2H {
    // dword 0

    // should be 0x34
    fis_type: u8,
    // four bitsfields merged into one
    // pmport is the port multiplier
    // rsv0 are the first two reserved bits
    // i is the interrupt bit
    // rsv1 is the 3rd reserved bit
    pmport_rsv0_i_rsv1: u8,
    // status register
    status: u8,
    // error register
    error: u8,

    // dword 1

    // lba low register, 7:0
    lba0: u8,
    // lba mid register, 15:8
    lba1: u8,
    // lba high register, 23:16
    lba2: u8,
    // device register
    device: u8,

    // dword 2

    // lba register, 31:24
    lba3: u8,
    // lba register, 39:32
    lba4: u8,
    // lba register, 47:40
    lba5: u8,
    // reserved
    rsv2: u8,

    // dword 3

    // count register, 7:0
    count_low: u8,
    // count register, 15:8
    count_high: u8,
    // reserved
    rsv3: [u8; 2],

    // dword 4
    rsv4: [u8; 4],
}

impl FisRegD2H {
    fn get_pmport(&self) -> u8 {
        self.pmport_rsv0_i_rsv1 & 0b0000_1111
    }

    fn get_interrupt_bit(&self) -> u8 {
        self.pmport_rsv0_i_rsv1 & 0b0100_0000
    }

    fn set_pmport(&mut self, val: u8) {
        self.pmport_rsv0_i_rsv1 = (self.pmport_rsv0_i_rsv1 & 0b1111_0000) | (val & 0b0000_1111);
    }

    fn set_interrupt_bit(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_i_rsv1 |= 0b0100_0000;
        } else {
            self.pmport_rsv0_i_rsv1 &= !0b0100_0000;
        }
    }
}

#[repr(C)]
struct FisData {
    // dword 0

    // should be 0x46
    fis_type: u8,
    // two bitfields merged into one,
    // contains port multiplier and reserved
    pmport_rsv0: u8,
    // reserved
    rsv1: [u8; 2],

    // dword 1 ~ N
    payload: [u32; 1],
}

impl FisData {
    fn get_pmport(&self) -> u8 {
        self.pmport_rsv0 & 0b0000_1111
    }

    fn set_pmport(&mut self, val: u8) {
        self.pmport_rsv0 = (self.pmport_rsv0 & 0b1111_0000) | (val & 0b0000_1111);
    }
}

#[repr(C)]
struct FisPIOSetup {
    // dword 0

    // should be 0x5F
    fis_type: u8,
    // five bitfields merged into one
    // pmport is the port multiplier
    // rsv0 is the first reserved bit
    // d is the data transfer direction
    // where 1 is device to host
    // i is the interrupt bit and
    // rsv1 is the second reserved bit
    pmport_rsv0_d_i_rsv1: u8,
    // status register
    status: u8,
    // error register
    error: u8,

    // dword 1

    // lba low register, 7:0
    lba0: u8,
    // lba mid register, 15:8
    lba1: u8,
    // lba high register, 23:16
    lba2: u8,
    // device register
    device: u8,

    // dword 2

    // lba register, 31:24
    lba3: u8,
    // lba register, 39:32
    lba4: u8,
    // lba register, 47:40
    lba5: u8,
    // reserved
    rsv2: u8,

    // dword 3

    // count register, 7:0
    count_low: u8,
    // count register, 15:8
    count_high: u8,
    // reserved
    rsv3: u8,
    // new value of status register
    e_status: u8,

    // dword 4

    // transfer count
    tc: u16,
    // reserved
    rsv4: [u8; 2],
}

impl FisPIOSetup {
    fn get_pmport(&self) -> u8 {
        self.pmport_rsv0_d_i_rsv1 & 0b0000_1111
    }

    fn get_direction(&self) -> u8 {
        self.pmport_rsv0_d_i_rsv1 & 0b0010_0000
    }

    fn get_interrupt_bit(&self) -> u8 {
        self.pmport_rsv0_d_i_rsv1 & 0b0100_0000
    }

    fn set_pmport(&mut self, val: u8) {
        self.pmport_rsv0_d_i_rsv1 = (self.pmport_rsv0_d_i_rsv1 & 0b1111_0000) | (val & 0b0000_1111);
    }

    fn set_direction(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_d_i_rsv1 |= 0b0010_0000;
        } else {
            self.pmport_rsv0_d_i_rsv1 &= !0b0010_0000;
        }
    }

    fn set_interrupt_bit(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_d_i_rsv1 |= 0b0100_0000;
        } else {
            self.pmport_rsv0_d_i_rsv1 &= !0b0100_0000;
        }
    }
}

#[repr(C)]
struct FisDMASetup {
    // dword 0

    // should be 0x41
    fis_type: u8,
    // five bitfields merged into one
    // pmport is the port multiplier
    // rsv0 is the first reserved bit
    // d is the data transfer direction,
    // where 1 is device to host,
    // i is the interrupt bit, and
    // a is auto-activate, which specifies
    // if DMA Activate FIS is needed.
    pmport_rsv0_d_i_a: u8,
    // reserved
    rsv1: [u8; 2],

    // dword 1 & 2

    // DMA buffer identifier. Used to
    // identify DMA buffer in host memory
    dma_buffer_id: u64,

    // dword 3

    // reserved
    rsv2: u32,

    // dword 4

    // byte offset into dma buffer,
    // first 2 bits must be zero
    dma_buffer_offset: u32,

    // dword 5

    // number of bytes to transfer,
    // bit zero must be set to zero
    transfer_count: u32,

    // dword 6

    // reserved
    rsv3: u32,
}

impl FisDMASetup {
    fn get_pmport(&self) -> u8 {
        self.pmport_rsv0_d_i_a & 0b0000_1111
    }

    fn get_direction(&self) -> u8 {
        self.pmport_rsv0_d_i_a & 0b0010_0000
    }

    fn get_interrupt_bit(&self) -> u8 {
        self.pmport_rsv0_d_i_a & 0b0100_0000
    }

    fn get_auto_activate(&self) -> u8 {
        self.pmport_rsv0_d_i_a & 0b1000_0000
    }

    fn set_pmport(&mut self, val: u8) {
        self.pmport_rsv0_d_i_a = (self.pmport_rsv0_d_i_a & 0b1111_0000) | (val & 0b0000_1111);
    }

    fn set_direction(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_d_i_a |= 0b0010_0000;
        } else {
            self.pmport_rsv0_d_i_a &= !0b0010_0000;
        }
    }

    fn set_interrupt_bit(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_d_i_a |= 0b0100_0000;
        } else {
            self.pmport_rsv0_d_i_a &= !0b0100_0000;
        }
    }

    fn set_auto_activate(&mut self, val: bool) {
        if val {
            self.pmport_rsv0_d_i_a |= 0b1000_0000;
        } else {
            self.pmport_rsv0_d_i_a &= !0b1000_0000;
        }
    }
}

pub fn init_ahci(controller: PciDevice) {}
