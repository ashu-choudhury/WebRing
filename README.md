# WebRing: Universal User-Mode Container Engine & Application Bundler for WebAssembly

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Runtime](https://img.shields.io/badge/runtime-Bun%20%7C%20Browser-black.svg?logo=bun)](https://bun.sh)
[![WASM Footprint](https://img.shields.io/badge/wasm%20footprint-168%20KB-success.svg)](#benchmarks)
[![Boot Time](https://img.shields.io/badge/boot%20latency-%3C1ms-brightgreen.svg)](#benchmarks)
[![Architecture](https://img.shields.io/badge/arch-RISC--V%2064%20(RV64GC)-orange.svg)](#architecture)
[![ISA Support](https://img.shields.io/badge/ISA-RV64I%2BM%2BA%2BF%2BD%2BC-purple.svg)](#instruction-set)

> **WebRing** is a headless Linux user-space ABI translation engine and zero-VM container runtime for WebAssembly, the browser, and the ultra-fast **Bun** runtime. Bringing the **"Wine approach"** to containers, WebRing intercepts Linux system calls and translates them in real time directly to WebAssembly and host primitives — executing authentic, unmodified Linux binaries with zero kernel virtualization overhead.

---

## 💡 The Universal Application Bundler Paradigm

### "Compile Once for Linux, Embed as an In-Process Library Anywhere with Bun"

Historically, running native Linux programs outside a server required heavy hypervisors (Docker Desktop, WSL2, QEMU, VirtualBox) or complex cross-compilation toolchains for every client operating system (Windows x64, macOS Apple Silicon, Android, iOS, Browser Emscripten).

**WebRing changes the distribution model entirely:**

```
+---------------------------------------------------------------------------------------------------+
|                                     ANY LINUX SOFTWARE                                            |
|                  (C / C++ / Rust / Go / Python / Musl / Alpine Linux / BusyBox / FFmpeg)          |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  | Compile once to static RISC-V 64 (`riscv64`)
                                                  v
+---------------------------------------------------------------------------------------------------+
|                                 .ring CONTAINER DISK BUNDLE                                       |
|               (Self-contained rootfs layer + manifest JSON: entrypoint, cmd, env, workdir)       |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  | Embed as a dependency via @webring/core
                                                  v
+---------------------------------------------------------------------------------------------------+
|                            EMBEDDABLE RUNTIME (168 KB WebAssembly Engine)                         |
|                                                                                                   |
|    🥟 Bun Runtime        🌐 Web Browsers       🖥️ Desktop Apps       ☁️ Serverless & Edge          |
|    (Native execution)    (Chrome, Firefox)     (Electron, Tauri)     (Cloudflare Workers)          |
|    Instant cold boot     1ms boot, 0 sandbox   No native rebuilds    Fastly, Wasmtime, Deno        |
+---------------------------------------------------------------------------------------------------+
```

Any developer can take an existing Linux CLI tool or microservice, package it with `webring pack`, and ship it as a standalone, in-process library that runs natively inside:
1. **Bun Runtime:** Ultra-fast, zero-overhead execution directly from terminal or backend services.
2. **Web Browsers (Client-side):** Runs directly in a Web Worker without any backend server.
3. **Desktop Apps (Electron / Tauri):** Eliminates multi-platform binary compilation.
4. **Mobile Apps (React Native / Flutter):** Runs Linux binaries inside iOS and Android apps.
5. **Edge Runtimes (Cloudflare Workers, Bun Edge):** Executes untrusted containers in isolated memory with sub-millisecond cold starts.

---

## ⚡ Performance & Comparison

| Metric | Full Virtual Machine (v86 / WebVM) | Docker / Containerd | WebRing (Bun / Browser) |
| :--- | :--- | :--- | :--- |
| **Boot Latency** | 5,000 – 30,000 ms | 1,000 – 3,000 ms | **< 1 ms (Instantaneous)** |
| **Engine Footprint** | 10 MB – 50 MB | > 500 MB (Host daemon) | **168 KB (.wasm)** |
| **Memory Overhead** | 64 MB – 512 MB | > 100 MB | **< 2 MB** |
| **Hardware Emulation** | BIOS, APIC, PCI, Timers, MMU | Host Linux Kernel | **None (Pure User-Mode ABI)** |
| **Privileges Required** | Hypervisor / VT-x | Root / daemon / WSL2 | **Unprivileged / In-Process** |
| **Execution Host** | Browser / x86 | Native Linux / VM | **Anywhere Bun & WASM run** |
| **Rootfs Format** | Raw disk images (ext4, qcow2) | OCI Layers | **Direct `.tar` & `.ring` Bundles** |

---

## 🏗️ Technical Architecture

WebRing separates container execution into four isolated layers:

```
+-------------------------------------------------------------------------------------------+
|                          HOST APPLICATION (Bun / TypeScript / Browser)                    |
|                                                                                           |
|   import { WebRingContainer } from '@webring/core';                                       |
|   const wasmBytes = new Uint8Array(await Bun.file('webring_core.wasm').arrayBuffer());    |
|   const container = await WebRingContainer.init({ wasmSource: wasmBytes });              |
|   const ringBytes = new Uint8Array(await Bun.file('alpine.ring').arrayBuffer());          |
|   const { process } = container.execBundle(ringBytes, 'busybox uname -a');                |
|   process.run();                                                                          |
+-------------------------------------------------------------------------------------------+
                                             |
                                             v
+-------------------------------------------------------------------------------------------+
|                           WEBRING ENGINE (crates/webring-core)                            |
|                                                                                           |
|  1. Virtual Memory (memory/):                                                             |
|     - 64-bit sparse page table with PagePermissions (R/W/X).                              |
|     - Dynamic brk heap tracking and mmap anonymous memory page allocator.                |
|                                                                                           |
|  2. ELF-64 Loader (elf/):                                                                 |
|     - Parses ELF headers and maps PT_LOAD executable segments.                            |
|     - Allocates Thread-Local Storage (PT_TLS) and initializes thread pointer (tp / x4).   |
|     - Constructs initial guest stack with argc, argv, envp, and Linux auxv descriptors.   |
|                                                                                           |
|  3. RV64GC CPU Engine (cpu/):                                                             |
|     - RV64I: Base 64-bit integer instructions.                                            |
|     - RV64M: Hardware integer multiplication and division.                                |
|     - RV64A: Atomic memory operations (LR, SC, AMOSWAP, AMOADD, AMOAND, AMOOR).           |
|     - RV64F & RV64D: IEEE-754 Single & Double precision floating-point arithmetic.        |
|     - FMA: Fused Multiply-Add (FMADD, FMSUB, FNMSUB, FNMADD).                             |
|     - RVC: Standard 16-bit compressed instruction decompressor.                           |
|                                                                                           |
|  4. Linux Syscall Layer (syscall/):                                                       |
|     - Intercepts user-mode ecall (opcode 0x73) and translates POSIX system calls.         |
|     - File descriptors: 0 (stdin), 1 (stdout), 2 (stderr), 3+ (virtual files & sockets).  |
|                                                                                           |
|  5. Virtual In-Memory Filesystem (vfs/):                                                  |
|     - In-memory hierarchical directory tree.                                              |
|     - Native USTAR Tar unpacker for mounting Docker and Alpine container layers.          |
+-------------------------------------------------------------------------------------------+
```

---

## 🧰 The `webring` CLI Toolchain (Powered by Bun)

Install globally via Bun:

```bash
bun add -g @webring/core
```

Or execute directly with `bun`:

```bash
# View engine specifications and ABI capabilities
bun packages/core/bin/webring.mjs info
```

### 1. Run Container Images or Raw ELF Binaries
```bash
# Execute a self-contained container bundle
bun packages/core/bin/webring.mjs run alpine.ring

# Execute with custom container command
bun packages/core/bin/webring.mjs run alpine.ring busybox echo "Hello from Bun!"

# Execute a raw RISC-V 64 ELF binary and mount a rootfs tarball
bun packages/core/bin/webring.mjs run /bin/busybox --rootfs alpine-rootfs.tar uname -a
```

### 2. Package Container Bundles (`.ring`)
```bash
# Package a rootfs folder into a .ring container bundle
bun packages/core/bin/webring.mjs pack ./rootfs -o myapp.ring --entrypoint /bin/busybox --cmd "sh"

# Inspect bundle metadata (entrypoint, cmd, environment, size)
bun packages/core/bin/webring.mjs inspect myapp.ring
```

### 3. Build Directly from Dockerfile
```bash
# Automatically builds for linux/riscv64 and exports to .ring bundle
bun packages/core/bin/webring.mjs build -f Dockerfile -t my-app -o my-app.ring
```

### 4. Export Standalone Web Application
```bash
# Generates a self-hosted single-page app containing the container
bun packages/core/bin/webring.mjs export-web myapp.ring -o ./public
```

---

## 📦 Container Bundle Specification (`.ring`)

A `.ring` file is a portable, zero-overhead container disk bundle format:

```
+---------------+---------------+--------------------+--------------------+-----------------------+
| Magic (4B)    | Version (2B)  | Manifest Len (4B)  | Manifest JSON (NB) | RootFS Tar Payload    |
| 'RING'        | 0x0001        | Big-Endian uint32  | UTF-8 JSON String  | POSIX USTAR Tarball   |
+---------------+---------------+--------------------+--------------------+-----------------------+
```

Example Manifest JSON:
```json
{
  "name": "alpine-custom",
  "version": "1.0.0",
  "arch": "riscv64",
  "entrypoint": "bin/busybox",
  "cmd": ["busybox", "uname", "-a"],
  "env": [
    "PATH=/bin:/usr/bin:/sbin:/usr/sbin",
    "TERM=xterm-256color",
    "HOME=/root"
  ],
  "workdir": "/root"
}
```

---

## 💻 Developer SDK (Bun & TypeScript)

```typescript
import { WebRingContainer } from '@webring/core';

// 1. Initialize engine using Bun's native high-performance file loader
const wasmBytes = new Uint8Array(await Bun.file('webring_core.wasm').arrayBuffer());
const container = await WebRingContainer.init({ wasmSource: wasmBytes });

// 2. Load container bundle (.ring)
const ringBytes = new Uint8Array(await Bun.file('alpine.ring').arrayBuffer());
const { process, manifest } = container.execBundle(ringBytes, 'busybox cat /etc/os-release');

// 3. Execute
const exitCode = process.run(20_000_000n);

// 4. Retrieve output
console.log(process.readStdoutText());
console.log(`Executed ${process.instructionsExecuted().toLocaleString()} instructions in container.`);

// 5. Clean up
process.destroy();
```

---

## 🖥️ Interactive In-Browser Playground

WebRing includes an in-browser Linux terminal running in a background Web Worker:

```bash
# Start local development server
python -m http.server 8080 --directory examples/browser
```

Navigate to: **[http://localhost:8080](http://localhost:8080)**

Features:
- **Genuine Alpine Linux 3.20:** Boots authentic `busybox` compiled for RISC-V 64.
- **Full Shell Interaction:** Run `uname -a`, `ls -la /`, `cat /etc/os-release`, `ip a`, `ping 1.1.1.1`, file creation, pipes, and redirects.
- **Mount Container Bundles:** Click **"📦 Mount .ring Container"** to upload and execute any custom `.ring` container bundle.
- **Live Throughput Counters:** Real-time instruction counts, latency, and MIPS throughput.

---

## 📋 Implemented Linux ABI Syscalls

| Syscall | Code | Description |
| :--- | :--- | :--- |
| `sys_getcwd` | 17 | Return current working directory |
| `sys_ioctl` | 29 | Query terminal window dimensions (`TIOCGWINSZ`) |
| `sys_openat` | 56 | Open files relative to directory file descriptor |
| `sys_close` | 57 | Close open file descriptors |
| `sys_getdents64` | 61 | Read directory entries (`struct linux_dirent64`) |
| `sys_lseek` | 62 | Reposition read/write file offset |
| `sys_read` | 63 | Read from files, stdin, and sockets |
| `sys_write` | 64 | Write to files, stdout, and sockets |
| `sys_writev` | 66 | Vectorized I/O writes (`struct iovec`) |
| `sys_newfstatat` | 79 | Get file status by pathname |
| `sys_fstat` | 80 | Get file status by file descriptor |
| `sys_exit` / `sys_exit_group` | 93 / 94 | Terminate container process |
| `sys_clock_gettime` | 113 | Retrieve POSIX clock time |
| `sys_uname` | 160 | Return Linux kernel information |
| `sys_gettimeofday` | 169 | Retrieve calendar time |
| `sys_getpid` / `sys_getuid` | 172 / 174 | Container process and root credentials |
| `sys_socket` / `sys_connect` | 198 / 203 | Transparent virtual socket networking |
| `sys_brk` | 214 | Adjust process heap breakpoint |
| `sys_mmap` | 222 | Map anonymous memory pages |
| `sys_getrandom` | 278 | Cryptographic entropy source for libc |

---

## 🧪 Testing & Verification with Bun

```bash
# Run all Bun integration test suites (E2E Container + Floating Point)
bun test/e2e.test.mjs
bun test/fp.test.mjs

# Or run via package test script:
bun run test

# Run all native Rust unit and integration test suites
cargo test --workspace

# Run WebAssembly compilation check
cargo check --target wasm32-unknown-unknown

# Run Floating-Point verification suite in Rust
cargo test --test fp_test

# Test real Alpine Linux BusyBox binary execution
cargo test --test busybox_debug
```

---

## 📄 License

Dual-licensed under either **MIT** or **Apache-2.0** at your option.
