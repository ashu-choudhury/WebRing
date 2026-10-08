#!/usr/bin/env node

/**
 * WebRing CLI: Run container executables and rootfs layers from the terminal.
 */

import { WebRingContainer } from '../dist/index.js';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/webring_core.wasm');

const args = process.argv.slice(2);

function printHelp() {
  console.log(`
WebRing CLI (v0.1.0)
Ultra-lightweight user-mode container runtime for WebAssembly

Usage:
  webring run <binary.elf> [--rootfs <layer.tar>] [--steps <max_steps>]
  webring info
  webring help

Options:
  --rootfs <path>     Path to OCI/Docker container rootfs tarball to mount
  --steps <count>     Maximum instructions to execute (default: 10,000,000)
`);
}

async function main() {
  if (args.length === 0 || args.includes('--help') || args.includes('-h') || args[0] === 'help') {
    printHelp();
    process.exit(0);
  }

  const cmd = args[0];

  if (cmd === 'info') {
    console.log(`
WebRing Runtime Info:
  Core Architecture:  RISC-V 64 User-Mode (RV64I + RV64M)
  Engine Size:        112 KB
  Boot Latency:       < 1 ms
  Filesystem:         Virtual VFS (USTAR Tar layer mounting supported)
  Syscalls:           Linux ABI translation (POSIX-compliant)
`);
    process.exit(0);
  }

  if (cmd === 'run') {
    const elfPath = args[1];
    if (!elfPath) {
      console.error('Error: Missing path to ELF binary.');
      process.exit(1);
    }

    let rootfsPath = null;
    let maxSteps = 10_000_000;

    for (let i = 2; i < args.length; i++) {
      if (args[i] === '--rootfs' && args[i + 1]) {
        rootfsPath = args[i + 1];
        i++;
      } else if (args[i] === '--steps' && args[i + 1]) {
        maxSteps = parseInt(args[i + 1], 10);
        i++;
      }
    }

    // Load WebRing wasm
    const wasmBytes = await fs.readFile(wasmPath);
    const container = await WebRingContainer.init({ wasmSource: wasmBytes });

    // Load binary
    const elfBytes = await fs.readFile(elfPath);
    const proc = container.exec(new Uint8Array(elfBytes));

    // Mount rootfs if provided
    if (rootfsPath) {
      const tarBytes = await fs.readFile(rootfsPath);
      const count = proc.mountTar(new Uint8Array(tarBytes));
      console.log(`[WebRing] Mounted ${count} entries from rootfs: ${rootfsPath}`);
    }

    const startTime = performance.now();
    const exitCode = proc.run(maxSteps);
    const duration = performance.now() - startTime;
    const instructions = proc.instructionsExecuted();

    // Stream output
    const stdout = proc.readStdoutText();
    const stderr = proc.readStderrText();

    if (stdout) process.stdout.write(stdout);
    if (stderr) process.stderr.write(stderr);

    const mips = duration > 0 ? (instructions / (duration / 1000) / 1_000_000).toFixed(2) : 'N/A';
    console.log(`\n[Process Exited] Code: ${exitCode} | Instructions: ${instructions.toLocaleString()} | Time: ${duration.toFixed(2)}ms | ${mips} MIPS`);

    proc.destroy();
    process.exit(exitCode);
  }

  console.error(`Unknown command: ${cmd}`);
  printHelp();
  process.exit(1);
}

main().catch((err) => {
  console.error('[WebRing Fatal Error]:', err.message);
  process.exit(1);
});
