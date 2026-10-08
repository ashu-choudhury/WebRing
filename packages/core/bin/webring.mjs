#!/usr/bin/env node

/**
 * WebRing CLI & Container Toolchain:
 * Build, pack, inspect, and run Linux container images inside WebAssembly.
 */

import { WebRingContainer, RingBundle } from '../dist/index.js';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(__dirname, '../../../target/wasm32-unknown-unknown/release/webring_core.wasm');

const args = process.argv.slice(2);

function printHelp() {
  console.log(`
WebRing CLI (v0.1.0)
Zero-VM Linux ABI Container Engine & Packaging Toolchain for WebAssembly

Usage:
  webring run <container.ring | binary.elf> [args...] [--rootfs <layer.tar>] [--steps <n>]
  webring pack <folder | rootfs.tar> -o <output.ring> [--entrypoint <bin>] [--cmd <args>]
  webring build <Dockerfile | dir> -o <output.ring> [-t <name>]
  webring inspect <container.ring>
  webring export-web <container.ring> -o <web_dir>
  webring info
  webring help

Commands:
  run          Execute a .ring container bundle or raw RISC-V 64 ELF binary
  pack         Package a rootfs directory or tarball into a self-contained .ring bundle
  build        Build a Dockerfile for riscv64 and package into a .ring container
  inspect      Display container manifest metadata and entrypoint of a .ring file
  export-web   Export a standalone browser web-app containing the container
  info         Display WebRing engine specifications and ABI capabilities
`);
}

/**
 * Creates a standard POSIX USTAR tar buffer from a local directory in pure JS.
 */
async function createTarFromDir(baseDir) {
  const buffers = [];

  async function walk(currentPath, relPath) {
    const entries = await fs.readdir(currentPath, { withFileTypes: true });
    for (const ent of entries) {
      const fullPath = path.join(currentPath, ent.name);
      const entryRel = relPath ? `${relPath}/${ent.name}` : ent.name;

      if (ent.isDirectory()) {
        buffers.push(createHeader(entryRel + '/', 0, '5'));
        await walk(fullPath, entryRel);
      } else if (ent.isFile()) {
        const fileBytes = await fs.readFile(fullPath);
        buffers.push(createHeader(entryRel, fileBytes.length, '0'));
        buffers.push(fileBytes);
        // Pad to 512 boundary
        const remainder = fileBytes.length % 512;
        if (remainder !== 0) {
          buffers.push(new Uint8Array(512 - remainder));
        }
      }
    }
  }

  function createHeader(name, size, typeflag) {
    const header = new Uint8Array(512);
    const enc = new TextEncoder();
    // name (100)
    header.set(enc.encode(name.slice(0, 100)), 0);
    // mode (8)
    header.set(enc.encode('0000755\0'), 100);
    // uid (8)
    header.set(enc.encode('0000000\0'), 108);
    // gid (8)
    header.set(enc.encode('0000000\0'), 116);
    // size (12 octal)
    const sizeOct = size.toString(8).padStart(11, '0') + '\0';
    header.set(enc.encode(sizeOct), 124);
    // mtime (12 octal)
    header.set(enc.encode('14000000000\0'), 136);
    // typeflag (1)
    header[156] = typeflag.charCodeAt(0);
    // magic (6) "ustar\0"
    header.set(enc.encode('ustar\0'), 257);
    // version (2) "00"
    header.set(enc.encode('00'), 263);

    // Compute checksum (sum of unsigned bytes with chksum field filled with spaces)
    header.fill(32, 148, 156);
    let chk = 0;
    for (let i = 0; i < 512; i++) chk += header[i];
    const chkOct = chk.toString(8).padStart(6, '0') + '\0 ';
    header.set(enc.encode(chkOct), 148);

    return header;
  }

  await walk(baseDir, '');
  // Two 512-byte zero blocks at EOF
  buffers.push(new Uint8Array(1024));

  // Merge buffers
  const total = buffers.reduce((acc, b) => acc + b.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const b of buffers) {
    out.set(b, off);
    off += b.length;
  }
  return out;
}

async function main() {
  if (args.length === 0 || args.includes('--help') || args.includes('-h') || args[0] === 'help') {
    printHelp();
    process.exit(0);
  }

  const cmd = args[0];

  if (cmd === 'info') {
    console.log(`
======================================================
  WebRing: Zero-VM Linux ABI Container Engine
======================================================
  Architecture:        RISC-V 64 User-Mode (RV64GC: RV64I + RV64M + RV64A + RV64F + RV64D + RVC)
  Engine Footprint:    ~168 KB (.wasm)
  Startup Overhead:    < 1 ms (Zero hardware boot overhead)
  Syscall Translation: Linux 6.1 POSIX ABI Translation
  Filesystem:          In-memory VFS with USTAR Tar layer mounting
  Execution Host:      Browser Web Worker, Wasmtime, Node.js, Bun, Cloudflare Workers
======================================================
`);
    process.exit(0);
  }

  if (cmd === 'inspect') {
    const ringPath = args[1];
    if (!ringPath) {
      console.error('Error: specify path to .ring container file.');
      process.exit(1);
    }
    const bytes = await fs.readFile(ringPath);
    const { manifest, rootfsTar } = RingBundle.parse(bytes);
    console.log(`
WebRing Container Inspection:
  Container Name:  ${manifest.name || 'unnamed'}
  Version:         ${manifest.version || '1.0.0'}
  Architecture:    ${manifest.arch}
  Entrypoint:      ${manifest.entrypoint}
  Default Command: ${(manifest.cmd || []).join(' ') || '(none)'}
  Environment:     ${(manifest.env || []).join(', ') || '(default)'}
  Working Dir:     ${manifest.workdir || '/root'}
  Rootfs Payload:  ${(rootfsTar.length / 1024 / 1024).toFixed(2)} MB
`);
    process.exit(0);
  }

  if (cmd === 'pack') {
    const inputPath = args[1];
    if (!inputPath) {
      console.error('Error: specify directory or rootfs tarball to pack.');
      process.exit(1);
    }

    let outputPath = 'container.ring';
    let entrypoint = '/bin/busybox';
    let defaultCmd = ['sh'];

    for (let i = 2; i < args.length; i++) {
      if (args[i] === '-o' && args[i + 1]) {
        outputPath = args[i + 1];
        i++;
      } else if (args[i] === '--entrypoint' && args[i + 1]) {
        entrypoint = args[i + 1];
        i++;
      } else if (args[i] === '--cmd' && args[i + 1]) {
        defaultCmd = args[i + 1].split(' ');
        i++;
      }
    }

    console.log(`[WebRing Pack] Packaging ${inputPath} -> ${outputPath}...`);
    let tarBytes;
    const stat = await fs.stat(inputPath);
    if (stat.isDirectory()) {
      tarBytes = await createTarFromDir(inputPath);
    } else {
      tarBytes = await fs.readFile(inputPath);
    }

    const manifest = {
      name: path.basename(outputPath, '.ring'),
      version: '1.0.0',
      arch: 'riscv64',
      entrypoint,
      cmd: defaultCmd,
      env: ['PATH=/bin:/usr/bin:/sbin:/usr/sbin', 'TERM=xterm-256color', 'HOME=/root'],
      workdir: '/root',
    };

    const bundleBytes = RingBundle.pack(manifest, tarBytes);
    await fs.writeFile(outputPath, bundleBytes);
    console.log(`[WebRing Pack] Successfully created container bundle: ${outputPath} (${(bundleBytes.length / 1024 / 1024).toFixed(2)} MB)`);
    process.exit(0);
  }

  if (cmd === 'build') {
    const target = args[1] || '.';
    let outputPath = 'app.ring';
    let tag = 'webring-image:latest';

    for (let i = 2; i < args.length; i++) {
      if (args[i] === '-o' && args[i + 1]) {
        outputPath = args[i + 1];
        i++;
      } else if (args[i] === '-t' && args[i + 1]) {
        tag = args[i + 1];
        i++;
      }
    }

    console.log(`[WebRing Build] Building Docker image for platform linux/riscv64...`);
    try {
      execSync(`docker buildx build --platform linux/riscv64 -t ${tag} ${target}`, { stdio: 'inherit' });
      const containerId = execSync(`docker create --platform linux/riscv64 ${tag}`, { encoding: 'utf8' }).trim();
      const tarPath = path.resolve(__dirname, `temp_rootfs_${Date.now()}.tar`);
      execSync(`docker export ${containerId} -o "${tarPath}"`, { stdio: 'inherit' });
      execSync(`docker rm ${containerId}`, { stdio: 'ignore' });

      const tarBytes = await fs.readFile(tarPath);
      await fs.unlink(tarPath);

      const manifest = {
        name: tag,
        version: '1.0.0',
        arch: 'riscv64',
        entrypoint: '/bin/sh',
        cmd: ['sh'],
        env: ['PATH=/bin:/usr/bin:/sbin:/usr/sbin', 'HOME=/root'],
        workdir: '/root',
      };

      const bundleBytes = RingBundle.pack(manifest, tarBytes);
      await fs.writeFile(outputPath, bundleBytes);
      console.log(`[WebRing Build] Container image successfully exported to ${outputPath}!`);
    } catch (e) {
      console.log(`[WebRing Build Notice] Docker buildx not available or failed (${e.message}). Falling back to direct folder packaging.`);
      const stat = await fs.stat(target);
      const tarBytes = stat.isDirectory() ? await createTarFromDir(target) : await fs.readFile(target);
      const manifest = {
        name: path.basename(outputPath, '.ring'),
        arch: 'riscv64',
        entrypoint: '/bin/busybox',
        cmd: ['sh'],
      };
      const bundle = RingBundle.pack(manifest, tarBytes);
      await fs.writeFile(outputPath, bundle);
      console.log(`[WebRing Build] Created ${outputPath} from directory contents.`);
    }
    process.exit(0);
  }

  if (cmd === 'export-web') {
    const ringPath = args[1];
    let outDir = 'web-export';
    for (let i = 2; i < args.length; i++) {
      if (args[i] === '-o' && args[i + 1]) {
        outDir = args[i + 1];
        i++;
      }
    }
    await fs.mkdir(outDir, { recursive: true });

    // Copy wasm & ring
    const wasmBytes = await fs.readFile(wasmPath);
    await fs.writeFile(path.join(outDir, 'webring_core.wasm'), wasmBytes);
    const ringBytes = await fs.readFile(ringPath);
    await fs.writeFile(path.join(outDir, 'container.ring'), ringBytes);

    const htmlContent = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>WebRing Container</title>
  <style>body { font-family: monospace; background: #0f172a; color: #f8fafc; padding: 20px; }</style>
</head>
<body>
  <h2>WebRing Container Browser Runtime</h2>
  <pre id="output">Loading container.ring...</pre>
  <script type="module">
    import { WebRingContainer, RingBundle } from './webring-bundle.js';
    const output = document.getElementById('output');
    try {
      const container = await WebRingContainer.init({ wasmSource: 'webring_core.wasm' });
      const ringResp = await fetch('container.ring');
      const ringBytes = new Uint8Array(await ringResp.arrayBuffer());
      const { process, manifest } = container.execBundle(ringBytes);
      output.textContent = 'Container loaded: ' + (manifest.name || 'WebRing') + '\\nBooting...\\n';
      const code = process.run(10000000n);
      output.textContent += process.readStdoutText() || '(Process finished with code ' + code + ')';
    } catch (err) {
      output.textContent = 'Error: ' + err.message;
    }
  </script>
</body>
</html>`;
    await fs.writeFile(path.join(outDir, 'index.html'), htmlContent);
    console.log(`[WebRing] Exported web application to ${outDir}/`);
    process.exit(0);
  }

  if (cmd === 'run') {
    const targetFile = args[1];
    if (!targetFile) {
      console.error('Error: specify container file or binary to run.');
      process.exit(1);
    }

    let rootfsPath = null;
    let maxSteps = 20_000_000n;
    const cmdArgs = [];

    for (let i = 2; i < args.length; i++) {
      if (args[i] === '--rootfs' && args[i + 1]) {
        rootfsPath = args[i + 1];
        i++;
      } else if (args[i] === '--steps' && args[i + 1]) {
        maxSteps = BigInt(args[i + 1]);
        i++;
      } else {
        cmdArgs.push(args[i]);
      }
    }

    const wasmBytes = await fs.readFile(wasmPath);
    const container = await WebRingContainer.init({ wasmSource: wasmBytes });

    const fileBytes = await fs.readFile(targetFile);
    let proc;
    const isRingBundle = fileBytes.length >= 4 &&
      fileBytes[0] === 0x52 && fileBytes[1] === 0x49 && fileBytes[2] === 0x4E && fileBytes[3] === 0x47;

    if (isRingBundle) {
      const overrideCmd = cmdArgs.length > 0 ? cmdArgs.join(' ') : undefined;
      const booted = container.execBundle(fileBytes, overrideCmd);
      proc = booted.process;
      console.log(`[WebRing] Booted container bundle "${booted.manifest.name || targetFile}"`);
    } else {
      const cmdLine = cmdArgs.length > 0 ? cmdArgs.join(' ') : undefined;
      proc = container.exec(new Uint8Array(fileBytes), cmdLine);
      if (rootfsPath) {
        const tarBytes = await fs.readFile(rootfsPath);
        const count = proc.mountTar(new Uint8Array(tarBytes));
        console.log(`[WebRing] Mounted ${count} files from ${rootfsPath}`);
      }
    }

    const t0 = performance.now();
    const exitCode = proc.run(maxSteps);
    const elapsed = performance.now() - t0;
    const inst = proc.instructionsExecuted();

    const stdout = proc.readStdoutText();
    const stderr = proc.readStderrText();
    if (stdout) process.stdout.write(stdout);
    if (stderr) process.stderr.write(stderr);

    const mips = elapsed > 0 ? (Number(inst) / (elapsed / 1000) / 1_000_000).toFixed(2) : 'N/A';
    console.log(`\n[Process Exited] Code: ${exitCode} | Instructions: ${inst.toLocaleString()} | Time: ${elapsed.toFixed(2)}ms | ${mips} MIPS`);

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
