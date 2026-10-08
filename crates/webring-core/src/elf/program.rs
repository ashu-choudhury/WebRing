//! ELF-64 Program Header parser.

use super::header::ElfError;
use crate::memory::PagePermissions;

pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_SHLIB: u32 = 5;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;

pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

#[derive(Debug, Clone)]
pub struct ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

impl ProgramHeader {
    pub fn parse(data: &[u8]) -> Result<Self, ElfError> {
        if data.len() < 56 {
            return Err(ElfError::BufferTooSmall);
        }

        let p_type = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let p_flags = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let p_offset = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let p_vaddr = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let p_paddr = u64::from_le_bytes(data[24..32].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let p_memsz = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let p_align = u64::from_le_bytes(data[48..56].try_into().unwrap());

        Ok(Self {
            p_type,
            p_flags,
            p_offset,
            p_vaddr,
            p_paddr,
            p_filesz,
            p_memsz,
            p_align,
        })
    }

    pub fn permissions(&self) -> PagePermissions {
        PagePermissions {
            read: (self.p_flags & PF_R) != 0,
            write: (self.p_flags & PF_W) != 0,
            execute: (self.p_flags & PF_X) != 0,
        }
    }
}
