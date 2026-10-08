//! High-level ELF-64 Loader & Process Initializer.

use super::header::{ElfError, ElfHeader};
use super::program::{ProgramHeader, PT_LOAD, PT_PHDR};
use crate::memory::{MemoryError, PagePermissions, VirtualMemory, PAGE_MASK, PAGE_SIZE};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoaderError {
    #[error("ELF format error: {0}")]
    Elf(#[from] ElfError),
    #[error("Memory error: {0}")]
    Memory(#[from] MemoryError),
    #[error("Executable has no PT_LOAD segments")]
    NoLoadSegments,
}

// Linux Auxiliary Vector Types
pub const AT_NULL: u64 = 0;
pub const AT_IGNORE: u64 = 1;
pub const AT_EXECFD: u64 = 2;
pub const AT_PHDR: u64 = 3;
pub const AT_PHENT: u64 = 4;
pub const AT_PHNUM: u64 = 5;
pub const AT_PAGESZ: u64 = 6;
pub const AT_BASE: u64 = 7;
pub const AT_FLAGS: u64 = 8;
pub const AT_ENTRY: u64 = 9;
pub const AT_NOTELF: u64 = 10;
pub const AT_UID: u64 = 11;
pub const AT_EUID: u64 = 12;
pub const AT_GID: u64 = 13;
pub const AT_EGID: u64 = 14;
pub const AT_PLATFORM: u64 = 15;
pub const AT_HWCAP: u64 = 16;
pub const AT_CLKTCK: u64 = 17;
pub const AT_RANDOM: u64 = 25;
pub const AT_EXECFN: u64 = 31;

#[derive(Debug)]
pub struct LoadedProcess {
    pub entry_point: u64,
    pub stack_pointer: u64,
    pub thread_pointer: u64,
    pub brk_base: u64,
    pub phdr_addr: u64,
    pub phnum: u16,
    pub phentsize: u16,
}

pub struct ElfLoader;

impl ElfLoader {
    /// Loads an ELF binary into virtual memory and sets up the initial process stack.
    pub fn load(
        binary: &[u8],
        memory: &mut VirtualMemory,
        args: &[String],
        env: &[String],
    ) -> Result<LoadedProcess, LoaderError> {
        let header = ElfHeader::parse(binary)?;

        let mut max_vaddr: u64 = 0;
        let mut phdr_addr: u64 = 0;

        let mut program_headers = Vec::with_capacity(header.ph_count as usize);

        for i in 0..header.ph_count {
            let offset = (header.ph_offset + (i as u64 * header.ph_entry_size as u64)) as usize;
            let end = offset + header.ph_entry_size as usize;
            if end > binary.len() {
                return Err(ElfError::BufferTooSmall.into());
            }

            let ph = ProgramHeader::parse(&binary[offset..end])?;
            if ph.p_type == PT_PHDR {
                phdr_addr = ph.p_vaddr;
            }

            if ph.p_type == PT_LOAD {
                let seg_start = ph.p_vaddr;
                let seg_mem_end = ph.p_vaddr + ph.p_memsz;
                if seg_mem_end > max_vaddr {
                    max_vaddr = seg_mem_end;
                }

                // Map pages for this segment
                memory.map_region(seg_start, ph.p_memsz, ph.permissions());

                // Copy file bytes
                let file_start = ph.p_offset as usize;
                let file_end = (ph.p_offset + ph.p_filesz) as usize;
                if file_end > binary.len() {
                    return Err(ElfError::BufferTooSmall.into());
                }

                if ph.p_filesz > 0 {
                    memory.write_bytes_raw(seg_start, &binary[file_start..file_end])?;
                }

                // Zero out any remaining BSS memory
                if ph.p_memsz > ph.p_filesz {
                    let bss_start = seg_start + ph.p_filesz;
                    let bss_len = ph.p_memsz - ph.p_filesz;
                    let zero_chunk = vec![0u8; bss_len as usize];
                    memory.write_bytes_raw(bss_start, &zero_chunk)?;
                }
            }

            program_headers.push(ph);
        }

        if max_vaddr == 0 {
            return Err(LoaderError::NoLoadSegments);
        }

        // If no explicit PT_PHDR segment was present, find the PT_LOAD segment that contains the program headers
        if phdr_addr == 0 {
            for ph in &program_headers {
                if ph.p_type == PT_LOAD
                    && header.ph_offset >= ph.p_offset
                    && header.ph_offset < ph.p_offset + ph.p_filesz
                {
                    phdr_addr = ph.p_vaddr + (header.ph_offset - ph.p_offset);
                    break;
                }
            }
        }

        // Set brk base right after the highest mapped segment
        let brk_base = (max_vaddr + PAGE_SIZE - 1) & PAGE_MASK;
        memory.set_brk_base(brk_base);

        // Map initial stack at 0x7fff_ffff_0000 (standard Linux 64-bit user stack)
        let stack_top: u64 = 0x7fff_ffff_f000;
        let stack_size: u64 = 8 * 1024 * 1024; // 8MB stack
        let stack_bottom = stack_top - stack_size;
        memory.map_region(stack_bottom, stack_size, PagePermissions::RW);

        // Initialize Thread Local Storage (TLS) / Thread Pointer (tp)
        let tls_addr: u64 = 0x7fff_fe00_0000;
        memory.map_region(tls_addr, 64 * 1024, PagePermissions::RW);
        for ph in &program_headers {
            if ph.p_type == 7 { // PT_TLS
                if ph.p_filesz > 0 {
                    let file_start = ph.p_offset as usize;
                    let file_end = (ph.p_offset + ph.p_filesz) as usize;
                    if file_end <= binary.len() {
                        memory.write_bytes_raw(tls_addr, &binary[file_start..file_end])?;
                    }
                }
            }
        }

        // Prepare stack content
        let sp = Self::setup_stack(
            memory,
            stack_top,
            &header,
            phdr_addr,
            args,
            env,
        )?;

        Ok(LoadedProcess {
            entry_point: header.entry_point,
            stack_pointer: sp,
            thread_pointer: tls_addr,
            brk_base,
            phdr_addr,
            phnum: header.ph_count,
            phentsize: header.ph_entry_size,
        })
    }

    /// Sets up argc, argv, envp, and auxv on the guest stack.
    fn setup_stack(
        memory: &mut VirtualMemory,
        mut sp: u64,
        header: &ElfHeader,
        phdr_addr: u64,
        args: &[String],
        env: &[String],
    ) -> Result<u64, MemoryError> {
        // 1. Write argument strings to the top of the stack
        let mut argv_addrs = Vec::with_capacity(args.len());
        for arg in args {
            let bytes = arg.as_bytes();
            sp -= bytes.len() as u64 + 1; // +1 for null terminator
            memory.write_bytes(sp, bytes)?;
            memory.write_u8(sp + bytes.len() as u64, 0)?;
            argv_addrs.push(sp);
        }

        // 2. Write environment strings to stack
        let mut env_addrs = Vec::with_capacity(env.len());
        for e in env {
            let bytes = e.as_bytes();
            sp -= bytes.len() as u64 + 1;
            memory.write_bytes(sp, bytes)?;
            memory.write_u8(sp + bytes.len() as u64, 0)?;
            env_addrs.push(sp);
        }

        // 3. Write 16 random bytes for AT_RANDOM
        sp -= 16;
        let random_bytes = [0x42u8; 16]; // Default seed
        memory.write_bytes(sp, &random_bytes)?;
        let random_ptr = sp;

        // 4. Align stack pointer to 16 bytes
        sp &= !0x0f;

        // 5. Construct the auxiliary vector
        let auxv: Vec<(u64, u64)> = vec![
            (AT_PAGESZ, PAGE_SIZE),
            (AT_PHDR, phdr_addr),
            (AT_PHENT, header.ph_entry_size as u64),
            (AT_PHNUM, header.ph_count as u64),
            (AT_ENTRY, header.entry_point),
            (AT_RANDOM, random_ptr),
            (AT_UID, 0),
            (AT_EUID, 0),
            (AT_GID, 0),
            (AT_EGID, 0),
            (AT_NULL, 0),
        ];

        // Auxiliary vector entries (each pair is 16 bytes)
        let auxv_size = (auxv.len() * 16) as u64;
        // envp array (pointers + NULL terminator = (env.len() + 1) * 8)
        let env_size = ((env_addrs.len() + 1) * 8) as u64;
        // argv array (pointers + NULL terminator = (args.len() + 1) * 8)
        let argv_size = ((argv_addrs.len() + 1) * 8) as u64;
        // argc (8 bytes)
        let total_words_size = 8 + argv_size + env_size + auxv_size;

        sp -= total_words_size;
        // Align to 16-byte boundary for ABI requirement
        sp &= !0x0f;

        let mut curr = sp;

        // Write argc
        memory.write_u64(curr, args.len() as u64)?;
        curr += 8;

        // Write argv pointers
        for addr in argv_addrs {
            memory.write_u64(curr, addr)?;
            curr += 8;
        }
        memory.write_u64(curr, 0)?; // argv NULL terminator
        curr += 8;

        // Write envp pointers
        for addr in env_addrs {
            memory.write_u64(curr, addr)?;
            curr += 8;
        }
        memory.write_u64(curr, 0)?; // envp NULL terminator
        curr += 8;

        // Write auxv pairs
        for (key, val) in auxv {
            memory.write_u64(curr, key)?;
            curr += 8;
            memory.write_u64(curr, val)?;
            curr += 8;
        }

        Ok(sp)
    }
}
