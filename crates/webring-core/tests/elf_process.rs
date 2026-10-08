use webring_core::WebRingProcess;

/// Minimal ELF-64 builder for testing
fn build_test_elf(instructions: &[u32]) -> Vec<u8> {
    let mut binary = Vec::new();

    let entry_point: u64 = 0x10000;
    let ph_offset: u64 = 64; // Program header immediately follows 64-byte ELF header
    let code_offset: u64 = 120; // 64 (ELF) + 56 (PHDR) = 120
    let code_size: u64 = (instructions.len() * 4) as u64;

    // 1. ELF Header (64 bytes)
    binary.extend_from_slice(&[0x7f, b'E', b'L', b'F']); // e_ident[EI_MAG0..3]
    binary.push(2); // ELFCLASS64
    binary.push(1); // ELFDATA2LSB (little-endian)
    binary.push(1); // EV_CURRENT
    binary.push(0); // ELFOSABI_NONE
    binary.extend_from_slice(&[0u8; 8]); // EI_PAD

    binary.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
    binary.extend_from_slice(&243u16.to_le_bytes()); // e_machine: EM_RISCV
    binary.extend_from_slice(&1u32.to_le_bytes()); // e_version: EV_CURRENT
    binary.extend_from_slice(&entry_point.to_le_bytes()); // e_entry
    binary.extend_from_slice(&ph_offset.to_le_bytes()); // e_phoff
    binary.extend_from_slice(&0u64.to_le_bytes()); // e_shoff
    binary.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    binary.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
    binary.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
    binary.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
    binary.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    binary.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    binary.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    assert_eq!(binary.len(), 64);

    // 2. Program Header (56 bytes): PT_LOAD segment
    binary.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
    binary.extend_from_slice(&(1 | 2 | 4u32).to_le_bytes()); // p_flags: PF_R | PF_W | PF_X
    binary.extend_from_slice(&code_offset.to_le_bytes()); // p_offset
    binary.extend_from_slice(&entry_point.to_le_bytes()); // p_vaddr
    binary.extend_from_slice(&entry_point.to_le_bytes()); // p_paddr
    binary.extend_from_slice(&code_size.to_le_bytes()); // p_filesz
    binary.extend_from_slice(&0x2000u64.to_le_bytes()); // p_memsz (allocate extra for bss/data)
    binary.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align: 4KB

    assert_eq!(binary.len(), 120);

    // 3. Instruction bytes
    for inst in instructions {
        binary.extend_from_slice(&inst.to_le_bytes());
    }

    binary
}

#[test]
fn test_elf_process_lifecycle_and_output() {
    // Instructions to write "WebRing Container Alive!\n" to stdout and exit(7)
    let msg = b"WebRing Container Alive!\n";

    // Instructions:
    // We will place the string right after the code at entry_point + code_len
    let code_inst_count = 10;
    let _msg_vaddr = 0x10000 + (code_inst_count * 4);

    let program = vec![
        // a7 = 64 (SYS_WRITE)
        0x13 | (17 << 7) | (64 << 20),
        // a0 = 1 (stdout)
        0x13 | (10 << 7) | (1 << 20),
        // lui a1, 0x10 (0x10000)
        0x37 | (11 << 7) | (0x10 << 12),
        // addi a1, a1, offset
        0x13 | (11 << 7) | (11 << 15) | (((code_inst_count * 4) as u32) << 20),
        // a2 = msg.len()
        0x13 | (12 << 7) | ((msg.len() as u32) << 20),
        // ecall (sys_write)
        0x73,
        // a7 = 93 (SYS_EXIT)
        0x13 | (17 << 7) | (93 << 20),
        // a0 = 7 (exit code 7)
        0x13 | (10 << 7) | (7 << 20),
        // ecall (sys_exit)
        0x73,
        // nop
        0x13,
    ];

    let mut elf_bytes = build_test_elf(&program);
    // Append the message bytes into the binary
    elf_bytes.extend_from_slice(msg);

    // Update p_filesz in program header to include the message
    let total_size = ((program.len() * 4) + msg.len()) as u64;
    elf_bytes[64 + 32..64 + 40].copy_from_slice(&total_size.to_le_bytes());

    // Load and run through WebRingProcess API!
    let args = vec!["/bin/container-test".to_string(), "arg1".to_string()];
    let env = vec!["PATH=/bin".to_string(), "TERM=xterm".to_string()];

    let mut process = WebRingProcess::new(&elf_bytes, args, env).unwrap();

    assert_eq!(process.is_exited(), false);

    // Run up to 1000 steps
    let exit_code = process.run(1000).unwrap();

    assert_eq!(exit_code, 7);
    assert_eq!(process.is_exited(), true);
    assert_eq!(process.exit_code(), 7);

    let stdout = process.read_stdout();
    let stdout_str = String::from_utf8(stdout).unwrap();
    assert_eq!(stdout_str, "WebRing Container Alive!\n");
}
