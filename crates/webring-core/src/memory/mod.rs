//! Virtual Memory Management for WebRing guest processes.

use std::collections::BTreeMap;
use thiserror::Error;

pub const PAGE_SIZE: u64 = 4096;
pub const PAGE_MASK: u64 = !(PAGE_SIZE - 1);

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Segmentation fault / Unmapped memory access at address 0x{0:x}")]
    Unmapped(u64),
    #[error("Permission denied at address 0x{0:x}")]
    PermissionDenied(u64),
    #[error("Invalid alignment at address 0x{0:x}")]
    Misaligned(u64),
    #[error("Out of memory")]
    OutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl PagePermissions {
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const R: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
}

#[derive(Debug, Clone)]
pub struct Page {
    pub data: Vec<u8>,
    pub perms: PagePermissions,
}

impl Page {
    pub fn new(perms: PagePermissions) -> Self {
        Self {
            data: vec![0u8; PAGE_SIZE as usize],
            perms,
        }
    }
}

/// Sparse Virtual Memory space for a 64-bit guest process.
#[derive(Debug, Default)]
pub struct VirtualMemory {
    pages: BTreeMap<u64, Page>,
    brk_start: u64,
    brk_current: u64,
    pub mmap_min_addr: u64,
}

impl VirtualMemory {
    pub fn new() -> Self {
        Self {
            pages: BTreeMap::new(),
            brk_start: 0x4000_0000,
            brk_current: 0x4000_0000,
            mmap_min_addr: 0x7000_0000_0000,
        }
    }

    pub fn set_brk_base(&mut self, base: u64) {
        let aligned = (base + PAGE_SIZE - 1) & PAGE_MASK;
        self.brk_start = aligned;
        self.brk_current = aligned;
    }

    pub fn brk_current(&self) -> u64 {
        self.brk_current
    }

    /// Implement the Linux `brk(2)` syscall logic.
    pub fn brk(&mut self, new_brk: u64) -> u64 {
        if new_brk == 0 || new_brk < self.brk_start {
            return self.brk_current;
        }

        let old_brk_page = (self.brk_current + PAGE_SIZE - 1) & PAGE_MASK;
        let new_brk_page = (new_brk + PAGE_SIZE - 1) & PAGE_MASK;

        if new_brk_page > old_brk_page {
            // Allocate new pages
            let mut addr = old_brk_page;
            while addr < new_brk_page {
                self.pages.entry(addr).or_insert_with(|| Page::new(PagePermissions::RW));
                addr += PAGE_SIZE;
            }
        } else if new_brk_page < old_brk_page {
            // Deallocate pages
            let mut addr = new_brk_page;
            while addr < old_brk_page {
                self.pages.remove(&addr);
                addr += PAGE_SIZE;
            }
        }

        self.brk_current = new_brk;
        self.brk_current
    }

    /// Map a region of memory with given permissions.
    pub fn map_region(&mut self, start: u64, length: u64, perms: PagePermissions) {
        let end = (start + length + PAGE_SIZE - 1) & PAGE_MASK;
        let mut curr = start & PAGE_MASK;
        while curr < end {
            self.pages.insert(curr, Page::new(perms));
            curr += PAGE_SIZE;
        }
    }

    /// Read a single byte.
    pub fn read_u8(&self, addr: u64) -> Result<u8, MemoryError> {
        let page_addr = addr & PAGE_MASK;
        let offset = (addr & (PAGE_SIZE - 1)) as usize;

        let page = self.pages.get(&page_addr).ok_or(MemoryError::Unmapped(addr))?;
        if !page.perms.read {
            return Err(MemoryError::PermissionDenied(addr));
        }
        Ok(page.data[offset])
    }

    /// Write a single byte.
    pub fn write_u8(&mut self, addr: u64, val: u8) -> Result<(), MemoryError> {
        let page_addr = addr & PAGE_MASK;
        let offset = (addr & (PAGE_SIZE - 1)) as usize;

        let page = self.pages.get_mut(&page_addr).ok_or(MemoryError::Unmapped(addr))?;
        if !page.perms.write {
            return Err(MemoryError::PermissionDenied(addr));
        }
        page.data[offset] = val;
        Ok(())
    }

    pub fn read_u16(&self, addr: u64) -> Result<u16, MemoryError> {
        let b0 = self.read_u8(addr)? as u16;
        let b1 = self.read_u8(addr + 1)? as u16;
        Ok(b0 | (b1 << 8))
    }

    pub fn write_u16(&mut self, addr: u64, val: u16) -> Result<(), MemoryError> {
        self.write_u8(addr, val as u8)?;
        self.write_u8(addr + 1, (val >> 8) as u8)?;
        Ok(())
    }

    pub fn read_u32(&self, addr: u64) -> Result<u32, MemoryError> {
        let b0 = self.read_u8(addr)? as u32;
        let b1 = self.read_u8(addr + 1)? as u32;
        let b2 = self.read_u8(addr + 2)? as u32;
        let b3 = self.read_u8(addr + 3)? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    pub fn write_u32(&mut self, addr: u64, val: u32) -> Result<(), MemoryError> {
        self.write_u8(addr, val as u8)?;
        self.write_u8(addr + 1, (val >> 8) as u8)?;
        self.write_u8(addr + 2, (val >> 16) as u8)?;
        self.write_u8(addr + 3, (val >> 24) as u8)?;
        Ok(())
    }

    pub fn read_u64(&self, addr: u64) -> Result<u64, MemoryError> {
        let low = self.read_u32(addr)? as u64;
        let high = self.read_u32(addr + 4)? as u64;
        Ok(low | (high << 32))
    }

    pub fn write_u64(&mut self, addr: u64, val: u64) -> Result<(), MemoryError> {
        self.write_u32(addr, val as u32)?;
        self.write_u32(addr + 4, (val >> 32) as u32)?;
        Ok(())
    }

    /// Read bytes from memory into a slice.
    pub fn read_bytes(&self, addr: u64, buf: &mut [u8]) -> Result<(), MemoryError> {
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = self.read_u8(addr + i as u64)?;
        }
        Ok(())
    }

    /// Write bytes from a slice into memory.
    pub fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), MemoryError> {
        for (i, &byte) in data.iter().enumerate() {
            self.write_u8(addr + i as u64, byte)?;
        }
        Ok(())
    }

    /// Write bytes ignoring page write permissions (used by ELF loader and debugger).
    pub fn write_bytes_raw(&mut self, addr: u64, data: &[u8]) -> Result<(), MemoryError> {
        for (i, &byte) in data.iter().enumerate() {
            let target = addr + i as u64;
            let page_addr = target & PAGE_MASK;
            let offset = (target & (PAGE_SIZE - 1)) as usize;
            let page = self.pages.get_mut(&page_addr).ok_or(MemoryError::Unmapped(target))?;
            page.data[offset] = byte;
        }
        Ok(())
    }

    /// Read a null-terminated C string from guest memory.
    pub fn read_c_string(&self, addr: u64) -> Result<String, MemoryError> {
        let mut bytes = Vec::new();
        let mut curr = addr;
        loop {
            let b = self.read_u8(curr)?;
            if b == 0 {
                break;
            }
            bytes.push(b);
            curr += 1;
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
