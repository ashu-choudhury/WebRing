import { WebRingContainer } from '../dist/index.js';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/webring_core.wasm');

console.log('--- WebRing End-to-End Test ---');
console.log('Loading WASM from:', wasmPath);

const wasmBytes = await fs.readFile(wasmPath);
const container = await WebRingContainer.init({ wasmSource: wasmBytes });

console.log('Container engine initialized successfully!');

// Build a real minimal RISC-V 64 ELF binary that writes to stdout and exits with code 42
function buildTestElf() {
  const instructions = [
    // a7 = 64 (SYS_WRITE)
    0x13 | (17 << 7) | (64 << 20),
    // a0 = 1 (stdout)
    0x13 | (10 << 7) | (1 << 20),
    // lui a1, 0x10 (0x10000)
    0x37 | (11 << 7) | (0x10 << 12),
    // addi a1, a1, 40 (offset of string at 0x10028)
    0x13 | (11 << 7) | (11 << 15) | (40 << 20),
    // a2 = length of message (31 bytes)
    0x13 | (12 << 7) | (31 << 20),
    // ecall (sys_write)
    0x73,
    // a7 = 93 (SYS_EXIT)
    0x13 | (17 << 7) | (93 << 20),
    // a0 = 42 (exit code 42)
    0x13 | (10 << 7) | (42 << 20),
    // ecall (sys_exit)
    0x73,
    // nop
    0x13,
  ];

  const msg = Buffer.from('Hello from WebRing Container!\n');
  const entryPoint = 0x10000n;
  const phOffset = 64n;
  const codeOffset = 120n;
  const totalFileSize = BigInt(120 + instructions.length * 4 + msg.length);

  const buf = Buffer.alloc(Number(totalFileSize));

  // ELF Header (64 bytes)
  buf.set([0x7f, 0x45, 0x4c, 0x46], 0); // \x7fELF
  buf.writeUInt8(2, 4); // 64-bit
  buf.writeUInt8(1, 5); // Little-endian
  buf.writeUInt8(1, 6); // Version 1
  buf.writeUInt8(0, 7); // OS ABI None
  buf.writeUInt16LE(2, 16); // ET_EXEC
  buf.writeUInt16LE(243, 18); // EM_RISCV
  buf.writeUInt32LE(1, 20); // EV_CURRENT
  buf.writeBigUInt64LE(entryPoint, 24); // e_entry
  buf.writeBigUInt64LE(phOffset, 32); // e_phoff
  buf.writeBigUInt64LE(0n, 40); // e_shoff
  buf.writeUInt32LE(0, 48); // e_flags
  buf.writeUInt16LE(64, 52); // e_ehsize
  buf.writeUInt16LE(56, 54); // e_phentsize
  buf.writeUInt16LE(1, 56); // e_phnum
  buf.writeUInt16LE(0, 58); // e_shentsize
  buf.writeUInt16LE(0, 60); // e_shnum
  buf.writeUInt16LE(0, 62); // e_shstrndx

  // Program Header (56 bytes at offset 64)
  buf.writeUInt32LE(1, 64); // PT_LOAD
  buf.writeUInt32LE(1 | 2 | 4, 68); // PF_R | PF_W | PF_X
  buf.writeBigUInt64LE(codeOffset, 72); // p_offset
  buf.writeBigUInt64LE(entryPoint, 80); // p_vaddr
  buf.writeBigUInt64LE(entryPoint, 88); // p_paddr
  buf.writeBigUInt64LE(BigInt(instructions.length * 4 + msg.length), 96); // p_filesz
  buf.writeBigUInt64LE(0x2000n, 104); // p_memsz
  buf.writeBigUInt64LE(0x1000n, 112); // p_align

  // Instructions
  let cur = 120;
  for (const inst of instructions) {
    buf.writeUInt32LE(inst, cur);
    cur += 4;
  }

  // String
  msg.copy(buf, cur);

  return new Uint8Array(buf);
}

const testBinary = buildTestElf();
console.log('Generated test container ELF binary (' + testBinary.length + ' bytes)');

console.log('Executing binary inside container...');
const proc = container.exec(testBinary);

const exitCode = proc.run(1000);
console.log('Process finished! Exit code:', exitCode);
console.log('Instructions executed:', proc.instructionsExecuted());

const stdout = proc.readStdoutText();
console.log('Container STDOUT output:');
console.log('----------------------------------------');
process.stdout.write(stdout);
console.log('----------------------------------------');

if (exitCode === 42 && stdout.includes('Hello from WebRing Container!')) {
  console.log('SUCCESS: WebRing container executed flawlessly!');
  process.exit(0);
} else {
  console.error('FAILURE: Unexpected result!');
  process.exit(1);
}
