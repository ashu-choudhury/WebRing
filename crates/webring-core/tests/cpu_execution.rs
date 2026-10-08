use webring_core::cpu::Rv64Cpu;
use webring_core::memory::{PagePermissions, VirtualMemory};
use webring_core::vfs::Vfs;

/// Helper to encode RISC-V instructions
mod asm {
    // lui rd, imm20
    pub fn lui(rd: u32, imm20: u32) -> u32 {
        0x37 | (rd << 7) | ((imm20 & 0xFFFFF) << 12)
    }

    // addi rd, rs1, imm
    pub fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
        let imm_u = (imm as u32) & 0xFFF;
        0x13 | (rd << 7) | (0b000 << 12) | (rs1 << 15) | (imm_u << 20)
    }

    // add rd, rs1, rs2
    pub fn add(rd: u32, rs1: u32, rs2: u32) -> u32 {
        0x33 | (rd << 7) | (0b000 << 12) | (rs1 << 15) | (rs2 << 20) | (0x00 << 25)
    }

    // sub rd, rs1, rs2
    pub fn sub(rd: u32, rs1: u32, rs2: u32) -> u32 {
        0x33 | (rd << 7) | (0b000 << 12) | (rs1 << 15) | (rs2 << 20) | (0x20 << 25)
    }

    // mul rd, rs1, rs2
    pub fn mul(rd: u32, rs1: u32, rs2: u32) -> u32 {
        0x33 | (rd << 7) | (0b000 << 12) | (rs1 << 15) | (rs2 << 20) | (0x01 << 25)
    }

    // bne rs1, rs2, offset
    pub fn bne(rs1: u32, rs2: u32, offset: i32) -> u32 {
        let imm = offset as u32;
        let imm12 = (imm >> 12) & 1;
        let imm11 = (imm >> 11) & 1;
        let imm10_5 = (imm >> 5) & 0x3F;
        let imm4_1 = (imm >> 1) & 0xF;
        0x63 | (imm11 << 7) | (imm4_1 << 8) | (0b001 << 12) | (rs1 << 15) | (rs2 << 20) | (imm10_5 << 25) | (imm12 << 31)
    }

    // sd rs2, offset(rs1)
    pub fn sd(rs2: u32, rs1: u32, offset: i32) -> u32 {
        let imm = (offset as u32) & 0xFFF;
        let imm4_0 = imm & 0x1F;
        let imm11_5 = (imm >> 5) & 0x7F;
        0x23 | (imm4_0 << 7) | (0b011 << 12) | (rs1 << 15) | (rs2 << 20) | (imm11_5 << 25)
    }

    // ld rd, offset(rs1)
    pub fn ld(rd: u32, rs1: u32, offset: i32) -> u32 {
        let imm = (offset as u32) & 0xFFF;
        0x03 | (rd << 7) | (0b011 << 12) | (rs1 << 15) | (imm << 20)
    }

    // ecall
    pub fn ecall() -> u32 {
        0x73
    }
}

#[test]
fn test_rv64_arithmetic_and_memory() {
    let mut mem = VirtualMemory::new();
    let mut vfs = Vfs::new();
    let mut cpu = Rv64Cpu::new();

    // Map code page at 0x10000
    let code_base = 0x10000;
    mem.map_region(code_base, 0x1000, PagePermissions::RWX);

    // Map data/stack page at 0x20000
    let data_base = 0x20000;
    mem.map_region(data_base, 0x1000, PagePermissions::RW);

    let program = [
        asm::addi(10, 0, 15),       // a0 = 15
        asm::addi(11, 0, 27),       // a1 = 27
        asm::add(12, 10, 11),       // a2 = a0 + a1 = 42
        asm::addi(13, 0, 2),        // a3 = 2
        asm::mul(14, 12, 13),       // a4 = 42 * 2 = 84
        asm::lui(1, (data_base >> 12) as u32), // x1 = data_base (0x20000)
        asm::sd(14, 1, 0),          // [data_base] = 84
        asm::ld(15, 1, 0),          // a5 = [data_base]
        // exit(a5):
        asm::addi(17, 0, 93),       // a7 = 93 (SYS_EXIT)
        asm::addi(10, 15, 0),       // a0 = a5 (84)
        asm::ecall(),               // exit(84)
    ];

    for (i, inst) in program.iter().enumerate() {
        mem.write_u32(code_base + (i as u64 * 4), *inst).unwrap();
    }

    cpu.set_entry(code_base, 0x30000);
    let exit_code = cpu.run(&mut mem, &mut vfs, 1000).unwrap();

    assert_eq!(exit_code, 84);
    assert!(cpu.exited);
    assert_eq!(cpu.regs.get(15), 84);
}

#[test]
fn test_rv64_loop_countdown() {
    let mut mem = VirtualMemory::new();
    let mut vfs = Vfs::new();
    let mut cpu = Rv64Cpu::new();

    let code_base = 0x10000;
    mem.map_region(code_base, 0x1000, PagePermissions::RWX);

    // Loop: counter in a0 starts at 10, decrement by 1 until 0
    let program = [
        asm::addi(10, 0, 10),      // 0x00: a0 = 10
        asm::addi(11, 0, 1),       // 0x04: a1 = 1
        // loop_start (0x08):
        asm::sub(10, 10, 11),      // 0x08: a0 = a0 - 1
        asm::bne(10, 0, -4),       // 0x0C: if a0 != 0 goto 0x08 (offset -4 bytes from 0x0C is 0x08)
        // exit(0):
        asm::addi(17, 0, 93),      // 0x10: a7 = 93 (SYS_EXIT)
        asm::addi(10, 0, 0),       // 0x14: a0 = 0
        asm::ecall(),              // 0x18: ecall
    ];

    for (i, inst) in program.iter().enumerate() {
        mem.write_u32(code_base + (i as u64 * 4), *inst).unwrap();
    }

    cpu.set_entry(code_base, 0x30000);
    let exit_code = cpu.run(&mut mem, &mut vfs, 1000).unwrap();

    assert_eq!(exit_code, 0);
    assert!(cpu.exited);
    assert_eq!(cpu.regs.get(10), 0);
}

#[test]
fn test_rv64_sys_write_stdout() {
    let mut mem = VirtualMemory::new();
    let mut vfs = Vfs::new();
    let mut cpu = Rv64Cpu::new();

    let code_base = 0x10000;
    mem.map_region(code_base, 0x1000, PagePermissions::RWX);

    let message_addr = 0x20000;
    mem.map_region(message_addr, 0x1000, PagePermissions::RW);
    let msg = b"Hello from WebRing!\n";
    mem.write_bytes(message_addr, msg).unwrap();

    // Call sys_write(fd=1, buf=message_addr, count=len)
    let program = [
        asm::addi(17, 0, 64),                 // a7 = 64 (SYS_WRITE)
        asm::addi(10, 0, 1),                  // a0 = 1 (stdout)
        asm::lui(11, (message_addr >> 12) as u32), // a1 = message_addr (0x20000)
        asm::addi(12, 0, msg.len() as i32),   // a2 = message length
        asm::ecall(),                         // sys_write()
        // exit(0)
        asm::addi(17, 0, 93),                 // a7 = 93 (SYS_EXIT)
        asm::addi(10, 0, 0),                  // a0 = 0
        asm::ecall(),                         // sys_exit()
    ];

    for (i, inst) in program.iter().enumerate() {
        mem.write_u32(code_base + (i as u64 * 4), *inst).unwrap();
    }

    cpu.set_entry(code_base, 0x30000);
    let exit_code = cpu.run(&mut mem, &mut vfs, 1000).unwrap();

    assert_eq!(exit_code, 0);
    assert!(cpu.exited);

    // Verify stdout captures the exact written text!
    let stdout = vfs.drain_stdout();
    assert_eq!(stdout, msg);
    assert_eq!(String::from_utf8(stdout).unwrap(), "Hello from WebRing!\n");
}
