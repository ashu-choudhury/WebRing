# WebRing: Architectural Vision & Design Manifesto

> **WebRing** is an ultra-lightweight, headless container runtime for WebAssembly and the modern browser, built on user-mode Linux ABI syscall translation rather than heavy virtual machine emulation.

---

## 1. Executive Summary

Traditional attempts to run containers or operating systems in the browser have almost universally relied on full hardware virtual machine emulators (such as v86 or QEMU-system). While functional, this approach is fundamentally misaligned with what containerization actually is:

1. **Containers are not virtual machines:** On Linux, Docker processes run directly on the host CPU in user space (Ring 3), isolated by kernel namespaces and cgroups, talking directly through the Linux system call interface.
2. **Hardware emulation is heavy and slow:** Simulating motherboards, BIOS, interrupt controllers (APIC/PIC), PCI buses, and booting a full monolithic Linux kernel incurs 5–30 second boot times, hundreds of megabytes of RAM overhead, and frequent UI thread stalls.
3. **Networking is typically crippled:** Virtual ethernet devices and nested NAT bridges inside emulators make browser networking fragile, slow, and prone to blocking.

**WebRing** rethinks browser-based execution from the ground up by taking the **"Wine approach"** for containers:
Instead of emulating a computer to boot a Linux kernel, WebRing acts as a high-speed **Linux ABI and Syscall Translation Engine** compiled to WebAssembly. ELF binaries from container images are loaded directly into memory and executed, with their system calls translated in real time into browser and WebAssembly primitives.

The result is instant execution (0ms boot), near-zero overhead, fully non-blocking operation in background workers, and transparent network socket communication.

---

## 2. Core Architectural Pillars

### 2.1 The "Wine" Approach (User-Mode ABI Emulation)
WebRing does not compile or run a guest Linux kernel. It implements the Linux user-space ABI (Application Binary Interface):
- **ELF64 Loader:** Parses standard ELF binaries (statically or dynamically linked), sets up the virtual address space, loads code/data segments, and populates the initial stack (arguments, environment variables, auxiliary vectors).
- **User-Mode CPU Core:** A streamlined, highly optimized user-mode CPU translation loop (targeting 64-bit architectures like RISC-V RV64 and x86-64). It executes application instructions without emulating privileged hardware modes (Ring 0 / Supervisor), hardware timers, or peripherals.
- **Syscall Dispatcher:** When the application issues a `syscall` or `ecall` instruction, WebRing intercepts the trap and directly handles it in WebAssembly/Rust (e.g., `sys_write` pushes to a stream buffer, `sys_brk` adjusts the heap pointer, `sys_openat` queries the VFS).

### 2.2 Headless-First Developer API
WebRing is designed primarily as a headless library and behind-the-scenes engine, not an opinionated UI:
- Developers integrate WebRing into web applications, IDEs, automation tools, or circuit/hardware design suites via a clean, modern TypeScript API.
- Process execution is completely decoupled from rendering. Engineers can attach to stdin/stdout/stderr via standard Web Streams, run commands programmatically, or connect their own UI (such as xterm.js or custom terminals) if desired.

### 2.3 Non-Blocking Multithreaded Engine
Heavy tasks—such as compilation, running shell scripts, opening web connections, or rendering complex models—must never freeze the user interface:
- **Worker Isolation:** The entire execution engine lives inside a dedicated Web Worker.
- **SharedArrayBuffer & Atomics:** Fast, lock-free ring buffers facilitate zero-copy I/O between the background worker and the host application.
- The host application's event loop and main thread remain completely free, guaranteeing a smooth 60fps experience even under 100% guest process CPU load.

### 2.4 Transparent Syscall-Level Networking
Rather than attempting to simulate virtual network cards (TAP/TUN) and complex packet routing inside the browser:
- WebRing intercepts socket syscalls directly (`sys_socket`, `sys_connect`, `sys_sendto`, `sys_recvfrom`, `sys_poll`).
- Virtual sockets are mapped to WebSocket or WebTransport connections multiplexed through a lightweight edge relay.
- Binaries running inside WebRing can make outbound TCP connections (HTTP requests, package managers, database clients) seamlessly without realizing they are running inside a browser sandbox.

### 2.5 High-Throughput Virtual Filesystem (VFS)
Container images are layered filesystems:
- **Layer 1 (In-Memory VFS):** Fast in-memory filesystem for temporary directories (`/tmp`, `/proc`, `/dev`) and standard input/output pipes.
- **Layer 2 (OPFS Persistence):** Persistent root filesystem backed by the Origin Private File System (OPFS) using synchronous access handles (`FileSystemSyncAccessHandle`) for near-native read/write speeds.
- **OCI / Docker Tar Mounting:** Ability to unpack or stream standard OCI/Docker container layer archives directly into the VFS.

---

## 3. System Architecture Diagram

```
+-------------------------------------------------------------------------+
|                        Host Application (Main Thread)                   |
|                                                                         |
|   import { WebRing } from '@webring/core';                              |
|   const container = await WebRing.create({ image: 'alpine:latest' });   |
|   const proc = await container.exec('/bin/sh', ['-l']);                 |
|   proc.stdout.pipeTo(myConsumerStream);                                 |
+-------------------------------------------------------------------------+
                                     |
              SharedArrayBuffer Ring Buffers / MessageChannel
                                     v
+-------------------------------------------------------------------------+
|                  WebRing Runtime Engine (Dedicated Web Worker)          |
|                                                                         |
|  +-------------------------------------------------------------------+  |
|  |                 Linux Syscall Translation Layer (Rust)            |  |
|  |                                                                   |  |
|  |   [ sys_read / sys_write ]  -->  Stdin / Stdout Ring Buffers      |  |
|  |   [ sys_openat / stat    ]  -->  VFS (OPFS + Memory)              |  |
|  |   [ sys_socket / connect ]  -->  WebSocket / WebTransport Relay   |  |
|  |   [ sys_mmap / sys_brk   ]  -->  Virtual Address Manager          |  |
|  +-------------------------------------------------------------------+  |
|                                     ^                                   |
|                        `syscall` / `ecall` Trap                         |
|                                     |                                   |
|  +-------------------------------------------------------------------+  |
|  |                     User-Mode CPU Execution Core                  |  |
|  |                                                                   |  |
|  |   - RV64 / x86-64 User-Mode Instruction Decoder & ALU             |  |
|  |   - General-Purpose Register File (x0 - x31, PC)                  |  |
|  |   - Linear Guest Memory Window (WASM Page Allocator)              |  |
|  +-------------------------------------------------------------------+  |
|                                     ^                                   |
|                           Loads & Maps Segments                         |
|                                     |                                   |
|  +-------------------------------------------------------------------+  |
|  |                       ELF-64 Binary Loader                        |  |
|  |   - Header validation (magic, architecture, endianness)           |  |
|  |   - Program Headers (PT_LOAD segments) allocation                 |  |
|  |   - Process stack layout (argc, argv, envp, auxv setup)           |  |
|  +-------------------------------------------------------------------+  |
+-------------------------------------------------------------------------+
```

---

## 4. The Developer Experience (API Design)

The primary interface of WebRing is idiomatic, clean TypeScript:

```typescript
import { WebRingEngine } from '@webring/core';

// 1. Initialize engine with optional network relay configuration
const engine = await WebRingEngine.init({
  networkRelay: 'wss://relay.example.com/tunnel',
  storage: 'opfs', // or 'memory'
});

// 2. Mount a container rootfs (OCI layer or tarball)
const container = await engine.createContainer({
  image: 'https://cdn.example.com/rootfs/alpine-rv64.tar.gz',
  environment: {
    PATH: '/bin:/usr/bin:/sbin:/usr/sbin',
    HOME: '/root',
    TERM: 'xterm-256color',
  },
});

// 3. Execute a command or interactive shell
const process = await container.exec('/bin/sh', ['-l']);

// 4. Handle streaming input and output
process.stdout.on('data', (bytes: Uint8Array) => {
  const text = new TextDecoder().decode(bytes);
  console.log('[CONTAINER STDOUT]:', text);
});

// 5. Send input commands directly
await process.stdin.write('echo "Hello from WebRing user-mode!"\n');
await process.stdin.write('uname -a\n');
await process.stdin.write('exit\n');

// 6. Wait for process exit
const exitCode = await process.wait();
console.log('Process terminated with code:', exitCode);
```

---

## 5. Technology Stack

- **Core Engine:** Written in Rust, compiled to `wasm32-unknown-unknown`.
- **Concurrency:** Web Workers with `SharedArrayBuffer` and `Atomics` for zero-latency synchronization.
- **Storage:** Origin Private File System (`FileSystemSyncAccessHandle`) and in-memory VFS.
- **Networking:** Virtual sockets over WebSockets / WebTransport.
- **Client Bindings:** TypeScript / ES Modules.

---

## 6. Phased Implementation Roadmap

### Phase 1: Core Foundation & ELF-64 Loader
- Initialize the Rust crate with WebAssembly build targets.
- Implement a robust ELF-64 parser (handling headers, program headers, `PT_LOAD` segments).
- Implement guest virtual memory space allocation and initial stack builder (`argc`, `argv`, `envp`, `auxv`).

### Phase 2: User-Mode CPU & Instruction Execution
- Implement the 64-bit user-mode CPU state (registers, program counter).
- Implement instruction decoding and execution loop for core instructions.
- Implement the syscall trap dispatch mechanism.

### Phase 3: Essential Linux Syscalls
- Implement standard file descriptor I/O (`sys_write`, `sys_read`, `sys_close`).
- Implement memory management syscalls (`sys_brk`, `sys_mmap`, `sys_munmap`).
- Implement process lifecycle syscalls (`sys_exit`, `sys_exit_group`, `sys_getpid`).
- **Milestone 1:** Execute a standalone compiled Linux "Hello World" ELF binary inside WebAssembly and capture its output in JavaScript.

### Phase 4: Virtual Filesystem (VFS)
- Build an in-memory VFS supporting hierarchical directory navigation.
- Implement path-based syscalls (`sys_openat`, `sys_fstat`, `sys_newfstatat`, `sys_getdents64`, `sys_readlinkat`).
- Add support for unpacking standard `.tar` root filesystems directly into the VFS.
- **Milestone 2:** Execute a statically linked shell (e.g., BusyBox `/bin/sh`) running built-in commands (`ls`, `pwd`, `cat`, `echo`).

### Phase 5: Transparent Sockets & Networking
- Implement socket syscalls (`sys_socket`, `sys_connect`, `sys_sendto`, `sys_recvfrom`).
- Multiplex network streams across a WebSocket relay client.
- **Milestone 3:** Run network-enabled tools (`curl`, `wget`) inside the container to fetch real web resources.

### Phase 6: Packaging, OPFS Storage & Public API
- Integrate OPFS for fast, persistent container layers.
- Package the TypeScript wrapper library (`@webring/core`) with full type definitions and Web Worker orchestration.
- Provide comprehensive test suites and benchmarks.
