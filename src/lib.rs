#![no_std]

/// Implementation Courtesy of MOROS.
/// Currently Only Supports ATA-PIO, with 24-bit LBA Addressing.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use bit_field::BitField;
use core::{hint::spin_loop, arch::asm};
use lazy_static::lazy_static;
use spin::Mutex;
// use x86_64::instructions::port::;
pub mod port;
use port::{Port, PortReadOnly, PortWriteOnly};

pub type BlockIndex = u32;

pub const ATA_BLOCK_SIZE: usize = 512;

fn sleep_ticks(ticks: usize) {
    for _ in 0..=ticks {
        unsafe {asm!("hlt", options(nomem, nostack, preserves_flags));}
    }
}

#[repr(u16)]
enum Command {
    Read = 0x20,
    Write = 0x30,
    Identify = 0xEC,
}

#[allow(dead_code)]
#[repr(usize)]
enum Status {
    ERR = 0,
    IDX = 1,
    CORR = 2,
    DRQ = 3,
    SRV = 4,
    DF = 5,
    RDY = 6,
    BSY = 7,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Bus {
    id: u8,
    irq: u8,

    data_register: Port<u16>,
    error_register: PortReadOnly<u8>,
    features_register: PortWriteOnly<u8>,
    sector_count_register: Port<u8>,
    lba0_register: Port<u8>,
    lba1_register: Port<u8>,
    lba2_register: Port<u8>,
    drive_register: Port<u8>,
    status_register: PortReadOnly<u8>,
    command_register: PortWriteOnly<u8>,

    alternate_status_register: PortReadOnly<u8>,
    control_register: PortWriteOnly<u8>,
    drive_blockess_register: PortReadOnly<u8>,
}

impl Bus {
    pub fn new(id: u8, io_base: u16, ctrl_base: u16, irq: u8) -> Self {
        Self {
            id, irq,

            data_register: Port::new(io_base + 0),
            error_register: PortReadOnly::new(io_base + 1),
            features_register: PortWriteOnly::new(io_base + 1),
            sector_count_register: Port::new(io_base + 2),
            lba0_register: Port::new(io_base + 3),
            lba1_register: Port::new(io_base + 4),
            lba2_register: Port::new(io_base + 5),
            drive_register: Port::new(io_base + 6),
            status_register: PortReadOnly::new(io_base + 7),
            command_register: PortWriteOnly::new(io_base + 7),

            alternate_status_register: PortReadOnly::new(ctrl_base + 0),
            control_register: PortWriteOnly::new(ctrl_base + 0),
            drive_blockess_register: PortReadOnly::new(ctrl_base + 1),
        }
    }

    fn reset(&mut self) {
        unsafe {
            self.control_register.write(4); // Set SRST bit
            sleep_ticks(2);
            self.control_register.write(0); // Then clear it
            sleep_ticks(2);
        }
    }

    fn wait(&mut self) {
        for _ in 0..4 { // Wait about 4 x 100 ns
            unsafe { self.alternate_status_register.read(); }
        }
    }

    fn write_command(&mut self, cmd: Command) {
        unsafe {
            self.command_register.write(cmd as u8);
        }
    }

    fn status(&mut self) -> u8 {
        unsafe { self.status_register.read() }
    }

    fn lba1(&mut self) -> u8 {
        unsafe { self.lba1_register.read() }
    }

    fn lba2(&mut self) -> u8 {
        unsafe { self.lba2_register.read() }
    }

    fn read_data(&mut self) -> u16 {
        unsafe { self.data_register.read() }
    }

    fn write_data(&mut self, data: u16) {
        unsafe { self.data_register.write(data) }
    }

    fn busy_loop(&mut self) {
        self.wait();
        let start = 0;
        while self.is_busy() {
            if 0 - start > 1 { // Hanged
                return self.reset();
            }

            spin_loop();
        }
    }

    fn is_busy(&mut self) -> bool {
        self.status().get_bit(Status::BSY as usize)
    }

    fn is_error(&mut self) -> bool {
        self.status().get_bit(Status::ERR as usize)
    }

    fn is_ready(&mut self) -> bool {
        self.status().get_bit(Status::RDY as usize)
    }

    fn select_drive(&mut self, drive: u8) {
        // Drive #0 (primary) = 0xA0
        // Drive #1 (secondary) = 0xB0
        let drive_id = 0xA0 | (drive << 4);
        unsafe {
            self.drive_register.write(drive_id);
        }
    }

    fn setup(&mut self, drive: u8, block: u32) {
        let drive_id = 0xE0 | (drive << 4);
        unsafe {
            self.drive_register.write(drive_id | ((block.get_bits(24..28) as u8) & 0x0F));
            self.sector_count_register.write(1);
            self.lba0_register.write(block.get_bits(0..8) as u8);
            self.lba1_register.write(block.get_bits(8..16) as u8);
            self.lba2_register.write(block.get_bits(16..24) as u8);
        }
    }

    pub fn identify_drive(&mut self, drive: u8) -> Option<[u16; 256]> {
        self.reset();
        self.wait();
        self.select_drive(drive);
        unsafe {
            self.sector_count_register.write(0);
            self.lba0_register.write(0);
            self.lba1_register.write(0);
            self.lba2_register.write(0);
        }

        self.write_command(Command::Identify);

        if self.status() == 0 {
            return None;
        }

        self.busy_loop();

        if self.lba1() != 0 || self.lba2() != 0 {
            return None;
        }

        for i in 0.. {
            if i == 256 {
                self.reset();
                return None;
            }
            if self.is_error() {
                return None;
            }
            if self.is_ready() {
                break;
            }
        }

        let mut res = [0; 256];
        for i in 0..256 {
            res[i] = self.read_data();
        }
        Some(res)
    }

    /// Read A single, 512-byte long slice from a given block
    /// panics if buf isn't EXACTLY 512 Bytes long;
    /// Example:
    /// ```rust
    /// // Read A Single block from a disk
    /// pub fn read_single() {
    ///     use x86_ata::{init, ATA_BLOCK_SIZE, read};
    ///     // 1. Initialise ATA Subsystem. (Perform Once, on boot)
    ///     init().expect("Failed To Start ATA...");  
    ///     // 2. Create a temporary buffer of size 512.
    ///     let mut buffer: [u8;ATA_BLOCK_SIZE] = [0; ATA_BLOCK_SIZE];
    ///     // 3. Pass the buffer over to the Subsystem, to be filled.
    ///     read(0, 0, 0, &mut buffer);
    /// }

    pub fn read(&mut self, drive: u8, block: BlockIndex, buf: &mut [u8]) {
        assert!(buf.len() == 512);
        //log!("Reading Block 0x{:8X}\n", block);
        //log!("{:?}", self);

        self.setup(drive, block);
        self.write_command(Command::Read);
        self.busy_loop();
        for i in (0..256).step_by(2) {
            let data = self.read_data();

            //log!("Read[{:08X}][{:02X}]: 0x{:04X}\n", block, i, data);
            buf[i + 0] = data.get_bits(0..8) as u8;
            buf[i + 1] = data.get_bits(8..16) as u8;
        }
    }

    /// Write A single, 512-byte long slice to a given block
    /// panics if buf isn't EXACTLY 512 Bytes long;
    /// Example:
    /// ```rust
    /// // Read A Single block from a disk
    /// pub fn write_single() {
    ///     use x86_ata::{init, ATA_BLOCK_SIZE, write};
    ///     // 1. Initialise ATA Subsystem. (Perform Once, on boot)
    ///     init().expect("Failed To Start ATA...");  
    ///     // 2. Create a temporary buffer of size 512.
    ///     let buffer: [u8;ATA_BLOCK_SIZE] = [0; ATA_BLOCK_SIZE];
    ///     // 3. Pass the buffer over to the Subsystem, to be filled.
    ///     write(0, 0, 0, &buffer);
    /// }

    pub fn write(&mut self, drive: u8, block: BlockIndex, buf: &[u8]) {
        assert!(buf.len() == 512);
        self.setup(drive, block);
        self.write_command(Command::Write);
        self.busy_loop();
        for i in 0..256 {
            let mut data = 0 as u16;
            data.set_bits(0..8, buf[i * 2] as u16);
            data.set_bits(8..16, buf[i * 2 + 1] as u16);

            //log!("Data: 0x{:04X} | {}{}    \n", data, buf[i * 2] as char, buf[i * 2 + 1] as char);

            self.write_data(data);
        }
        self.busy_loop();
    }
}

lazy_static! {
    pub static ref BUSES: Mutex<Vec<Bus>> = Mutex::new(Vec::new());
}

fn disk_size(sectors: u32) -> (u32, String) {
    let bytes = sectors * 512;
    if bytes >> 20 < 1000 {
        (bytes >> 20, String::from("MB"))
    } else {
        (bytes >> 30, String::from("GB"))
    }
}



pub fn list() -> Vec<(u8, u8, String, String, u32, String, u32)> {
    let mut buses = BUSES.lock();
    let mut res = Vec::new();
    for bus in 0..2 {
        for drive in 0..2 {
            if let Some(buf) = buses[bus as usize].identify_drive(drive) {
                let mut serial = String::new();
                for i in 10..20 {
                    for &b in &buf[i].to_be_bytes() {
                        serial.push(b as char);
                    }
                }
                serial = serial.trim().into();
                let mut model = String::new();
                for i in 27..47 {
                    for &b in &buf[i].to_be_bytes() {
                        model.push(b as char);
                    }
                }
                model = model.trim().into();
                let sectors = (buf[61] as u32) << 16 | (buf[60] as u32);
                let (size, unit) = disk_size(sectors);
                res.push((bus, drive, model, serial, size, unit, sectors));
            }
        }
    }
    res
}

/// Identify a specific drive on a bus, format: (bus, drive, model, serial. size, unit, sectors) 
pub fn indentify_drive(bus : u8, drive : u8) -> Option<(u8, u8, String, String, u32, String, u32)> {
    let mut buses = BUSES.lock();
            if let Some(buf) = buses[bus as usize].identify_drive(drive) {
                let mut serial = String::new();
                for i in 10..20 {
                    for &b in &buf[i].to_be_bytes() {
                        serial.push(b as char);
                    }
                }
                serial = serial.trim().into();
                let mut model = String::new();
                for i in 27..47 {
                    for &b in &buf[i].to_be_bytes() {
                        model.push(b as char);
                    }
                }
                model = model.trim().into();
        let sectors = (buf[61] as u32) << 16 | (buf[60] as u32);
        let (size, unit) = disk_size(sectors);
        Some((bus, drive, model, serial, size, unit, sectors))
    } else {
        None
    } 
}

pub fn read(bus: u8, drive: u8, block: BlockIndex, buf: &mut [u8]) {
    let mut buses = BUSES.lock();
    //log!("Reading Block 0x{:08X}\n", block);
    buses[bus as usize].read(drive, block, buf);
}

pub fn write(bus: u8, drive: u8, block: BlockIndex, buf : &[u8]) {
    let mut buses = BUSES.lock();
    //log!("Writing Block 0x{:08X}\n", block);
    buses[bus as usize].write
    (drive, block, buf);
}



pub fn drive_is_present(bus : usize) -> bool {
    unsafe {BUSES.lock()[bus].status_register.read() != 0xFF}
}



pub fn init() -> Result<(), ()> {
    {
        let mut buses = BUSES.lock();
        buses.push(Bus::new(0, 0x1F0, 0x3F6, 14));
        buses.push(Bus::new(1, 0x170, 0x376, 15));
    }
    Ok(())
}