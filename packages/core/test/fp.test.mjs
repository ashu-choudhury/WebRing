import { WebRingContainer } from '../dist/index.js';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/webring_core.wasm');

console.log('--- WebRing Floating-Point (RV64F & RV64D) Bun Test ---');

const wasmBytes = await fs.readFile(wasmPath);
const container = await WebRingContainer.init({ wasmSource: wasmBytes });

/**
 * Builds a valid ELF-64 binary with RV64FD floating point instructions.
 * Computes:
 *   f1 = 4.0
 *   f2 = 2.5
 *   f3 = f1 + f2 = 6.5
 *   f4 = f1 * f2 = 10.0
 *   f5 = sqrt(f1) = 2.0
 *   a0 = fcvt.l.d(f5) = 2
 *   exit(a0) -> exit code 2
 */
function buildFpElf() {
  // Float constant 4.0 (double precision): 0x4010_0000_0000_0000
  // Float constant 2.5 (double precision): 0x4004_0000_0000_0000
  const floatDataOffset = 24 * 4; // 96 bytes

  const insts = [
    // 0: lui t0, 0x10 (t0 = 0x10000, base of text/data segment)
    0x37 | (5 << 7) | (0x10 << 12),
    // 1: addi t0, t0, floatDataOffset
    0x13 | (5 << 7) | (5 << 15) | (floatDataOffset << 20),
    // 2: fld f1, 0(t0)  (load 4.0) -> opcode 0x07, funct3=011, rd=1, rs1=5, imm=0
    0x07 | (1 << 7) | (0b011 << 12) | (5 << 15) | (0 << 20),
    // 3: fld f2, 8(t0)  (load 2.5) -> opcode 0x07, funct3=011, rd=2, rs1=5, imm=8
    0x07 | (2 << 7) | (0b011 << 12) | (5 << 15) | (8 << 20),
    // 4: fadd.d f3, f1, f2 (f3 = 6.5) -> opcode 0x53, rd=3, rs1=1, rs2=2, funct7=0b0000001
    0x53 | (3 << 7) | (0 << 12) | (1 << 15) | (2 << 20) | (0b0000001 << 25),
    // 5: fmul.d f4, f1, f2 (f4 = 10.0) -> opcode 0x53, rd=4, rs1=1, rs2=2, funct7=0b0001001
    0x53 | (4 << 7) | (0 << 12) | (1 << 15) | (2 << 20) | (0b0001001 << 25),
    // 6: fsqrt.d f5, f1 (f5 = sqrt(4.0) = 2.0) -> opcode 0x53, rd=5, rs1=1, funct7=0b0101101
    0x53 | (5 << 7) | (0 << 12) | (1 << 15) | (0 << 20) | (0b0101101 << 25),
    // 7: fcvt.l.d a0, f5 (a0 = 2) -> opcode 0x53, rd=10, rs1=5, rs2=0b00010, funct7=0b1100001
    0x53 | (10 << 7) | (0 << 12) | (5 << 15) | (0b00010 << 20) | (0b1100001 << 25),
    // 8: addi a7, zero, 93 (sys_exit)
    0x13 | (17 << 7) | (0 << 15) | (93 << 20),
    // 9: ecall
    0x73,
  ];

  // Align to 24 instructions (96 bytes)
  while (insts.length < 24) {
    insts.push(0x13); // nop (addi x0, x0, 0)
  }

  const codeBytes = Buffer.alloc(insts.length * 4);
  for (let i = 0; i < insts.length; i++) {
    codeBytes.writeUInt32LE(insts[i] >>> 0, i * 4);
  }

  // Float data: 4.0 and 2.5
  const floatBuf = Buffer.alloc(16);
  floatBuf.writeDoubleLE(4.0, 0);
  floatBuf.writeDoubleLE(2.5, 8);

  const payload = Buffer.concat([codeBytes, floatBuf]);

  // Construct minimal valid ELF-64 header
  const elfHeader = Buffer.alloc(64);
  elfHeader.write('\x7FELF', 0);
  elfHeader[4] = 2; // 64-bit
  elfHeader[5] = 1; // Little-endian
  elfHeader[6] = 1; // Current version
  elfHeader[7] = 0; // System V ABI
  elfHeader.writeUInt16LE(2, 16); // ET_EXEC
  elfHeader.writeUInt16LE(0xF3, 18); // EM_RISCV
  elfHeader.writeUInt32LE(1, 20); // Version
  elfHeader.writeBigUInt64LE(0x10000n, 24); // Entry point
  elfHeader.writeBigUInt64LE(64n, 32); // Program header offset
  elfHeader.writeUInt16LE(64, 52); // ELF header size
  elfHeader.writeUInt16LE(56, 54); // Program header entry size
  elfHeader.writeUInt16LE(1, 56); // 1 program header

  // Program header (PT_LOAD)
  const progHeader = Buffer.alloc(56);
  progHeader.writeUInt32LE(1, 0); // PT_LOAD
  progHeader.writeUInt32LE(7, 4); // PF_R | PF_W | PF_X
  progHeader.writeBigUInt64LE(120n, 8); // Offset in file where code begins (64 + 56 = 120)
  progHeader.writeBigUInt64LE(0x10000n, 16); // Virtual address
  progHeader.writeBigUInt64LE(0x10000n, 24); // Physical address
  const payloadSize = BigInt(payload.length);
  progHeader.writeBigUInt64LE(payloadSize, 32); // File size
  progHeader.writeBigUInt64LE(payloadSize + 0x1000n, 40); // Memory size
  progHeader.writeBigUInt64LE(0x1000n, 48); // Alignment

  return Buffer.concat([elfHeader, progHeader, payload]);
}

const elfBytes = buildFpElf();
console.log(`Generated RV64FD ELF binary (${elfBytes.length} bytes)`);

const proc = container.exec(new Uint8Array(elfBytes));
const exitCode = proc.run(100_000n);
const insts = proc.instructionsExecuted();

console.log(`Execution completed!`);
console.log(`Exit code: ${exitCode} (Expected: 2)`);
console.log(`Instructions executed: ${insts}`);

if (exitCode === 2) {
  console.log('✅ SUCCESS: RV64FD Floating-Point (FADD.D, FMUL.D, FSQRT.D, FCVT.L.D) verified under Bun!');
} else {
  console.error(`❌ FAILED: Unexpected exit code: ${exitCode}`);
  process.exit(1);
}
