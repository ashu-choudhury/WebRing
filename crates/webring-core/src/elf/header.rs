//! ELF-64 File Header parser.

use thiserror::Error;

pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_LSB: u8 = 1; // Little Endian

pub const EM_RISCV: u16 = 243;
pub const EM_X86_64: u16 = 62;

#[derive(Error, Debug)]
pub enum ElfError {
    #[error("Invalid ELF magic numbers: expected [0x7f, 'E', 'L', 'F'], got {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("Unsupported ELF class (must be 64-bit): {0}")]
    UnsupportedClass(u8),
    #[error("Unsupported ELF endianness (must be little-endian): {0}")]
    UnsupportedEndian(u8),
    #[error("Buffer too small to contain ELF header")]
    BufferTooSmall,
    #[error("Unsupported machine architecture: {0}")]
    UnsupportedMachine(u16),
}

#[derive(Debug, Clone)]
pub struct ElfHeader {
    pub class: u8,
    pub endian: u8,
    pub os_abi: u8,
    pub elf_type: u16,
    pub machine: u16,
    pub version: u32,
    pub entry_point: u64,
    pub ph_offset: u64,
    pub sh_offset: u64,
    pub flags: u32,
    pub header_size: u16,
    pub ph_entry_size: u16,
    pub ph_count: u16,
    pub sh_entry_size: u16,
    pub sh_count: u16,
    pub sh_str_index: u16,
}

impl ElfHeader {
    pub fn parse(data: &[u8]) -> Result<Self, ElfError> {
        if data.len() < 64 {
            return Err(ElfError::BufferTooSmall);
        }

        let magic = [data[0], data[1], data[2], data[3]];
        if magic != ELF_MAGIC {
            return Err(ElfError::InvalidMagic(magic));
        }

        let class = data[4];
        if class != ELF_CLASS_64 {
            return Err(ElfError::UnsupportedClass(class));
        }

        let endian = data[5];
        if endian != ELF_DATA_LSB {
            return Err(ElfError::UnsupportedEndian(endian));
        }

        let os_abi = data[7];
        let elf_type = u16::from_le_bytes([data[16], data[17]]);
        let machine = u16::from_le_bytes([data[18], data[19]]);
        let version = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        let entry_point = u64::from_le_bytes(data[24..32].try_into().unwrap());
        let ph_offset = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let sh_offset = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let flags = u32::from_le_bytes(data[48..52].try_into().unwrap());
        let header_size = u16::from_le_bytes([data[52], data[53]]);
        let ph_entry_size = u16::from_le_bytes([data[54], data[55]]);
        let ph_count = u16::from_le_bytes([data[56], data[57]]);
        let sh_entry_size = u16::from_le_bytes([data[58], data[59]]);
        let sh_count = u16::from_le_bytes([data[60], data[61]]);
        let sh_str_index = u16::from_le_bytes([data[62], data[63]]);

        Ok(Self {
            class,
            endian,
            os_abi,
            elf_type,
            machine,
            version,
            entry_point,
            ph_offset,
            sh_offset,
            flags,
            header_size,
            ph_entry_size,
            ph_count,
            sh_entry_size,
            sh_count,
            sh_str_index,
        })
    }
}
