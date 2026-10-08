use webring_core::cpu::rv64::Rv64Cpu;
use webring_core::memory::{PagePermissions, VirtualMemory};
use webring_core::vfs::Vfs;

#[test]
fn test_rv64_floating_point() {
    let mut cpu = Rv64Cpu::new();
    let mut mem = VirtualMemory::new();
    let mut vfs = Vfs::new();

    // Map 64KB code page
    mem.map_region(0x1000, 0x10000, PagePermissions::RWX);

    // Setup initial float registers:
    // f1 = 3.5 (f64), f2 = 2.0 (f64)
    let f1_val = 3.5f64.to_bits();
    let f2_val = 2.0f64.to_bits();
    cpu.regs.set_f(1, f1_val);
    cpu.regs.set_f(2, f2_val);

    // Let's assemble:
    // 1. FADD.D f3, f1, f2 (f3 = 5.5) -> funct7=0b0000001, rs2=2, rs1=1, funct3=0, rd=3, opcode=0x53
    // 2. FMUL.D f4, f1, f2 (f4 = 7.0) -> funct7=0b0000001 (wait funct7=0b0001001 for FMUL.D)
    // 3. FCVT.L.D x5, f3 (x5 = 5) -> funct7=0b1100001, rs2=0b00010, rs1=3, rd=5, opcode=0x53
    let fadd_d = 0x53 | (3 << 7) | (0 << 12) | (1 << 15) | (2 << 20) | (0b0000001 << 25);
    let fmul_d = 0x53 | (4 << 7) | (0 << 12) | (1 << 15) | (2 << 20) | (0b0001001 << 25);
    let fcvt_l_d = 0x53 | (5 << 7) | (0 << 12) | (3 << 15) | (0b00010 << 20) | (0b1100001 << 25);

    mem.write_u32(0x1000, fadd_d).unwrap();
    mem.write_u32(0x1004, fmul_d).unwrap();
    mem.write_u32(0x1008, fcvt_l_d).unwrap();

    cpu.regs.pc = 0x1000;

    // Step 1: FADD.D
    cpu.step(&mut mem, &mut vfs).unwrap();
    let res_add = f64::from_bits(cpu.regs.get_f(3));
    assert_eq!(res_add, 5.5);

    // Step 2: FMUL.D
    cpu.step(&mut mem, &mut vfs).unwrap();
    let res_mul = f64::from_bits(cpu.regs.get_f(4));
    assert_eq!(res_mul, 7.0);

    // Step 3: FCVT.L.D
    cpu.step(&mut mem, &mut vfs).unwrap();
    assert_eq!(cpu.regs.get(5), 5);
}
