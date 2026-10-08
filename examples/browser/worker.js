// WebRing Background Worker: Runs container CPU loop off the main thread

let wasmInstance = null;
let currentProcPtr = 0;

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
        const response = await fetch(payload.wasmUrl);
        const result = await WebAssembly.instantiateStreaming(response, importObject);
        wasmInstance = result.instance;
        self.postMessage({ type: 'INIT_DONE' });
        break;
      }

      case 'EXEC': {
        if (!wasmInstance) throw new Error('WASM engine not initialized');

        const { binaryBytes, maxSteps } = payload;
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
            mips: (instCount / (durationMs / 1000) / 1_000_000).toFixed(2),
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
