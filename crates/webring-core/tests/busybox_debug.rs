use webring_core::cpu::Rv64Cpu;
use webring_core::elf::ElfLoader;
use webring_core::memory::VirtualMemory;
use webring_core::vfs::Vfs;

#[test]
fn test_load_and_step_real_busybox() {
    let busybox_bytes = match std::fs::read("examples/browser/busybox") {
        Ok(b) => b,
        Err(_) => match std::fs::read("../../examples/browser/busybox") {
            Ok(b) => b,
            Err(e) => {
                println!("Skipping: busybox not found: {}", e);
                return;
            }
        },
    };

    let mut mem = VirtualMemory::new();
    let args = vec!["busybox".to_string(), "cat".to_string(), "/etc/os-release".to_string()];
    let env = vec!["PATH=/bin:/usr/bin".to_string(), "TERM=xterm".to_string()];

    let header = webring_core::elf::ElfHeader::parse(&busybox_bytes).unwrap();
    println!("ELF header: ph_offset = 0x{:x}, ph_count = {}, ph_entry_size = {}", header.ph_offset, header.ph_count, header.ph_entry_size);
    for i in 0..header.ph_count {
        let off = (header.ph_offset + i as u64 * header.ph_entry_size as u64) as usize;
        let ph = webring_core::elf::ProgramHeader::parse(&busybox_bytes[off..off+header.ph_entry_size as usize]).unwrap();
        println!("PH[{}]: type=0x{:x}, vaddr=0x{:x}, paddr=0x{:x}, filesz=0x{:x}, memsz=0x{:x}, offset=0x{:x}, flags=0x{:x}",
            i, ph.p_type, ph.p_vaddr, ph.p_paddr, ph.p_filesz, ph.p_memsz, ph.p_offset, ph.p_flags);
    }

    let loaded = ElfLoader::load(&busybox_bytes, &mut mem, &args, &env).unwrap();

    let mut cpu = Rv64Cpu::new();
    cpu.set_entry(loaded.entry_point, loaded.stack_pointer);

    // Map a 64KB TLS/TCB block
    let tls_addr = 0x7fff_fe00_0000;
    mem.map_region(tls_addr, 64 * 1024, webring_core::memory::PagePermissions::RW);
    // Copy PT_TLS image if present
    for i in 0..header.ph_count {
        let off = (header.ph_offset + i as u64 * header.ph_entry_size as u64) as usize;
        let ph = webring_core::elf::ProgramHeader::parse(&busybox_bytes[off..off+header.ph_entry_size as usize]).unwrap();
        if ph.p_type == 7 { // PT_TLS
            println!("Copying PT_TLS (filesz 0x{:x}, memsz 0x{:x}) to TLS area...", ph.p_filesz, ph.p_memsz);
            let file_off = ph.p_offset as usize;
            let tls_data = &busybox_bytes[file_off..file_off + ph.p_filesz as usize];
            mem.write_bytes(tls_addr, tls_data).unwrap();
        }
    }
    cpu.regs.set(4, tls_addr);
    println!("Set tp (x4) = 0x{:x}", tls_addr);

    println!("--- Disassembly of 0x10430..0x10460 ---");
    let mut cur = 0x10430;
    while cur < 0x10460 {
        let raw = mem.read_u32(cur).unwrap_or(0);
        let is_rvc = (raw & 0x3) != 0x3;
        if is_rvc {
            let inst16 = (raw & 0xffff) as u16;
            let dec = webring_core::cpu::rvc::decompress_rvc(inst16);
            println!("0x{:x}: [16-bit] 0x{:04x} -> dec 0x{:08x?}", cur, inst16, dec);
            cur += 2;
        } else {
            println!("0x{:x}: [32-bit] 0x{:08x}", cur, raw);
            cur += 4;
        }
    }

    let mut vfs = Vfs::new();

    // Run up to 1,000,000 steps
    let mut steps = 0;
    while !cpu.exited && steps < 1_000_000 {
        match cpu.step(&mut mem, &mut vfs) {
            Ok(running) => {
                if !running {
                    println!("CPU halted / process exited cleanly at step {}! Code: {}", steps, cpu.exit_code);
                    break;
                }
            }
            Err(e) => {
                println!("CPU fault at step {}: {:?}, PC: 0x{:x}", steps, e, cpu.regs.pc);
                break;
            }
        }
        steps += 1;
    }

    println!("Executed {} steps! PC: 0x{:x}, exited: {}", steps, cpu.regs.pc, cpu.exited);
    let stdout = vfs.drain_stdout();
    let stderr = vfs.drain_stderr();
    println!("Captured STDOUT (len {}):\n{}", stdout.len(), String::from_utf8_lossy(&stdout));
    println!("Captured STDERR (len {}):\n{}", stderr.len(), String::from_utf8_lossy(&stderr));
}
