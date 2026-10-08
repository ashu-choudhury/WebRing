import { WebRingContainer } from '../dist/index.js';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/webring_core.wasm');

console.log('--- WebRing End-to-End Container Test ---');
const wasmBytes = await fs.readFile(wasmPath);
const container = await WebRingContainer.init({ wasmSource: wasmBytes });
console.log('Container engine initialized successfully!');

// Build a test tar archive containing /etc/container.txt
function buildTar(files) {
  let chunks = [];
  for (const file of files) {
    const header = Buffer.alloc(512);
    const nameBuf = Buffer.from(file.path.replace(/^\//, ''));
    nameBuf.copy(header, 0);

    const modeStr = Buffer.from('0000644\0');
    modeStr.copy(header, 100);

    const contentBuf = Buffer.from(file.content);
    const sizeOctal = Buffer.from(contentBuf.length.toString(8).padStart(11, '0') + '\0');
    sizeOctal.copy(header, 124);

    header[156] = 48; // '0' = regular file
    Buffer.from('ustar\0').copy(header, 257);

    chunks.push(header);
    chunks.push(contentBuf);

    const padLen = (contentBuf.length + 511) % 512;
    const pad = 512 - padLen;
    if (pad > 0 && pad < 512) {
      chunks.push(Buffer.alloc(pad));
    }
  }

  chunks.push(Buffer.alloc(1024)); // EOF
  return Buffer.concat(chunks);
}

// Build an ELF that opens /etc/container.txt via sys_openat, reads it via sys_read, writes to stdout via sys_write, and exits(0)
function buildCatElf() {
  const pathStr = Buffer.from('/etc/container.txt\0');
  const pathOffset = 25 * 4; // 25 instructions * 4 bytes = 100

  const instructions = [
    // a7 = 56 (openat)
    0x13 | (17 << 7) | (56 << 20),
    // a0 = -100 (AT_FDCWD)
    0x13 | (10 << 7) | ((-100 & 0xFFF) << 20),
    // lui a1, 0x10 (0x10000)
    0x37 | (11 << 7) | (0x10 << 12),
    // addi a1, a1, pathOffset
    0x13 | (11 << 7) | (11 << 15) | (pathOffset << 20),
    // a2 = 0 (flags)
    0x13 | (12 << 7) | (0 << 20),
    // ecall -> fd in a0
    0x73,
    // s1 (x9) = fd
    0x13 | (9 << 7) | (10 << 15) | (0 << 20),

    // a7 = 63 (read)
    0x13 | (17 << 7) | (63 << 20),
    // a0 = fd (x9)
    0x13 | (10 << 7) | (9 << 15) | (0 << 20),
    // a1 = 0x10500 (inside mapped PT_LOAD segment)
    0x37 | (11 << 7) | (0x10 << 12),
    0x13 | (11 << 7) | (11 << 15) | (0x500 << 20),
    // a2 = 128
    0x13 | (12 << 7) | (128 << 20),
    // ecall -> bytes_read in a0
    0x73,

    // write(stdout=1, buf=0x10500, count=a0)
    // a2 = count (a0)
    0x13 | (12 << 7) | (10 << 15) | (0 << 20),
    // a7 = 64 (write)
    0x13 | (17 << 7) | (64 << 20),
    // a0 = 1 (stdout)
    0x13 | (10 << 7) | (1 << 20),
    // a1 = 0x10500
    0x37 | (11 << 7) | (0x10 << 12),
    0x13 | (11 << 7) | (11 << 15) | (0x500 << 20),
    // ecall
    0x73,

    // close(fd)
    0x13 | (17 << 7) | (57 << 20),
    // a0 = fd (x9)
    0x13 | (10 << 7) | (9 << 15) | (0 << 20),
    // ecall
    0x73,

    // exit(0)
    0x13 | (17 << 7) | (93 << 20),
    0x13 | (10 << 7) | (0 << 20),
    0x73,
  ];

  const entryPoint = 0x10000n;
  const phOffset = 64n;
  const codeOffset = 120n;
  const totalFileSize = BigInt(120 + instructions.length * 4 + pathStr.length);

  const buf = Buffer.alloc(Number(totalFileSize));

  // ELF Header (64 bytes)
  buf.set([0x7f, 0x45, 0x4c, 0x46], 0);
  buf.writeUInt8(2, 4); // 64-bit
  buf.writeUInt8(1, 5); // Little-endian
  buf.writeUInt8(1, 6);
  buf.writeUInt16LE(2, 16); // ET_EXEC
  buf.writeUInt16LE(243, 18); // EM_RISCV
  buf.writeUInt32LE(1, 20);
  buf.writeBigUInt64LE(entryPoint, 24);
  buf.writeBigUInt64LE(phOffset, 32);
  buf.writeUInt16LE(64, 52);
  buf.writeUInt16LE(56, 54);
  buf.writeUInt16LE(1, 56);

  // Program Header (56 bytes)
  buf.writeUInt32LE(1, 64); // PT_LOAD
  buf.writeUInt32LE(7, 68); // PF_R | PF_W | PF_X
  buf.writeBigUInt64LE(codeOffset, 72);
  buf.writeBigUInt64LE(entryPoint, 80);
  buf.writeBigUInt64LE(entryPoint, 88);
  buf.writeBigUInt64LE(BigInt(instructions.length * 4 + pathStr.length), 96);
  buf.writeBigUInt64LE(0x4000n, 104);
  buf.writeBigUInt64LE(0x1000n, 112);

  let cur = 120;
  for (const inst of instructions) {
    buf.writeUInt32LE(inst >>> 0, cur);
    cur += 4;
  }
  pathStr.copy(buf, cur);

  return new Uint8Array(buf);
}

const tarData = buildTar([
  { path: 'etc/container.txt', content: 'Container Layer File: Verified in WebRing VFS!\n' }
]);

const elf = buildCatElf();
console.log('Created container ELF binary (' + elf.length + ' bytes)');
console.log('Created container tar layer (' + tarData.length + ' bytes)');

const proc = container.exec(elf);
const mounted = proc.mountTar(new Uint8Array(tarData));
console.log('Mounted', mounted, 'files from container tar layer into VFS');

const exitCode = proc.run(5000);
console.log('Process finished with exit code:', exitCode);

const output = proc.readStdoutText();
console.log('Container STDOUT output:');
console.log('----------------------------------------');
process.stdout.write(output);
console.log('----------------------------------------');

if (exitCode === 0 && output.includes('Container Layer File: Verified in WebRing VFS!')) {
  console.log('SUCCESS: Container tar layer mounted and read via real Linux syscalls!');
  process.exit(0);
} else {
  console.error('FAILURE: Unexpected output!');
  process.exit(1);
}
