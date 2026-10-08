// WebRing Background Worker: Runs container CPU loop & VFS off the main thread

let wasmInstance = null;
let currentProcPtr = 0;
let busyboxBytes = null;

const noop = () => 0;
const importObject = {
  __wbindgen_placeholder__: new Proxy({}, { get: () => noop }),
  __wbindgen_externref_xform__: new Proxy({}, { get: () => noop }),
  env: {},
};

self.onmessage = async (e) => {
  const { type, payload } = e.data;

  try {
    switch (type) {
      case 'INIT': {
        const [wasmResp, bbResp] = await Promise.all([
          fetch(payload.wasmUrl),
          fetch('busybox').catch(() => null),
        ]);
        const result = await WebAssembly.instantiateStreaming(wasmResp, importObject);
        wasmInstance = result.instance;
        if (bbResp && bbResp.ok) {
          busyboxBytes = new Uint8Array(await bbResp.arrayBuffer());
        }
        self.postMessage({ type: 'INIT_DONE', payload: { hasBusybox: !!busyboxBytes } });
        break;
      }

      case 'EXEC': {
        if (!wasmInstance) throw new Error('WASM engine not initialized');

        const { binaryBytes, maxSteps, tarBytes } = payload;
        const exports = wasmInstance.exports;

        // Allocate and copy binary
        const binPtr = exports.webring_alloc(binaryBytes.length);
        new Uint8Array(exports.memory.buffer, binPtr, binaryBytes.length).set(binaryBytes);

        // Create process
        currentProcPtr = exports.webring_create(binPtr, binaryBytes.length);
        exports.webring_dealloc(binPtr, binaryBytes.length);

        if (currentProcPtr === 0) {
          throw new Error('Failed to load container ELF binary');
        }

        // If a container tar layer was provided, mount it into VFS!
        let mountedEntries = 0;
        if (tarBytes && tarBytes.length > 0) {
          const tarPtr = exports.webring_alloc(tarBytes.length);
          new Uint8Array(exports.memory.buffer, tarPtr, tarBytes.length).set(tarBytes);
          mountedEntries = exports.webring_mount_tar(currentProcPtr, tarPtr, tarBytes.length);
          exports.webring_dealloc(tarPtr, tarBytes.length);
        }

        const startTime = performance.now();
        const exitCode = exports.webring_run(currentProcPtr, BigInt(maxSteps || 10_000_000));
        const durationMs = performance.now() - startTime;
        const instCount = Number(exports.webring_instructions_executed(currentProcPtr));

        // Read stdout
        const stdoutLen = exports.webring_read_stdout_len(currentProcPtr);
        let stdoutText = '';
        if (stdoutLen > 0) {
          const outPtr = exports.webring_alloc(stdoutLen);
          const actualRead = exports.webring_read_stdout(currentProcPtr, outPtr, stdoutLen);
          const rawBytes = new Uint8Array(exports.memory.buffer, outPtr, actualRead);
          stdoutText = new TextDecoder().decode(rawBytes);
          exports.webring_dealloc(outPtr, stdoutLen);
        }

        // Clean up
        exports.webring_destroy(currentProcPtr);
        currentProcPtr = 0;

        self.postMessage({
          type: 'EXEC_DONE',
          payload: {
            exitCode,
            stdout: stdoutText,
            instructions: instCount,
            durationMs,
            mips: durationMs > 0 ? (instCount / (durationMs / 1000) / 1_000_000).toFixed(2) : 'Infinity',
            mountedEntries,
          },
        });
        break;
      }

      case 'RUN_CMD': {
        if (!wasmInstance) throw new Error('WASM engine not initialized');
        if (!busyboxBytes) {
          const resp = await fetch('busybox');
          busyboxBytes = new Uint8Array(await resp.arrayBuffer());
        }

        const { cmd, maxSteps } = payload;
        const exports = wasmInstance.exports;

        // Allocate and copy busybox binary
        const binPtr = exports.webring_alloc(busyboxBytes.length);
        new Uint8Array(exports.memory.buffer, binPtr, busyboxBytes.length).set(busyboxBytes);

        // Command string
        const cmdStr = cmd.startsWith('busybox ') ? cmd : 'busybox ' + cmd;
        const cmdEncoded = new TextEncoder().encode(cmdStr);
        const cmdPtr = exports.webring_alloc(cmdEncoded.length);
        new Uint8Array(exports.memory.buffer, cmdPtr, cmdEncoded.length).set(cmdEncoded);

        currentProcPtr = exports.webring_create_args(binPtr, busyboxBytes.length, cmdPtr, cmdEncoded.length);
        exports.webring_dealloc(binPtr, busyboxBytes.length);
        exports.webring_dealloc(cmdPtr, cmdEncoded.length);

        if (currentProcPtr === 0) {
          throw new Error('Failed to load container ELF process');
        }

        const startTime = performance.now();
        const exitCode = exports.webring_run(currentProcPtr, BigInt(maxSteps || 2_000_000));
        const durationMs = performance.now() - startTime;
        const instCount = Number(exports.webring_instructions_executed(currentProcPtr));

        // Read stdout
        const stdoutLen = exports.webring_read_stdout_len(currentProcPtr);
        let stdoutText = '';
        if (stdoutLen > 0) {
          const outPtr = exports.webring_alloc(stdoutLen);
          const actualRead = exports.webring_read_stdout(currentProcPtr, outPtr, stdoutLen);
          const rawBytes = new Uint8Array(exports.memory.buffer, outPtr, actualRead);
          stdoutText = new TextDecoder().decode(rawBytes);
          exports.webring_dealloc(outPtr, stdoutLen);
        }

        // Read stderr
        const stderrLen = exports.webring_read_stderr_len(currentProcPtr);
        let stderrText = '';
        if (stderrLen > 0) {
          const errPtr = exports.webring_alloc(stderrLen);
          const actualRead = exports.webring_read_stderr(currentProcPtr, errPtr, stderrLen);
          const rawBytes = new Uint8Array(exports.memory.buffer, errPtr, actualRead);
          stderrText = new TextDecoder().decode(rawBytes);
          exports.webring_dealloc(errPtr, stderrLen);
        }

        exports.webring_destroy(currentProcPtr);
        currentProcPtr = 0;

        self.postMessage({
          type: 'CMD_DONE',
          payload: {
            cmd,
            exitCode,
            stdout: stdoutText,
            stderr: stderrText,
            instructions: instCount,
            durationMs,
            mips: durationMs > 0 ? (instCount / (durationMs / 1000) / 1_000_000).toFixed(2) : 'Infinity',
          },
        });
        break;
      }

      default:
        console.warn('Unknown worker message type:', type);
    }
  } catch (err) {
    self.postMessage({ type: 'ERROR', error: err.message });
  }
};
