/**
 * Raw WebAssembly Interface for WebRing.
 * Provides low-level binding to webring_core.wasm via direct memory manipulation.
 */

export interface WebRingWasmExports {
  memory: WebAssembly.Memory;
  webring_alloc(size: number): number;
  webring_dealloc(ptr: number, size: number): void;
  webring_create(binaryPtr: number, binaryLen: number): number;
  webring_create_args(binaryPtr: number, binaryLen: number, cmdPtr: number, cmdLen: number): number;
  webring_mount_tar(procPtr: number, tarPtr: number, tarLen: number): number;
  webring_step(procPtr: number): number;
  webring_run(procPtr: number, maxSteps: bigint): number;
  webring_write_stdin(procPtr: number, dataPtr: number, dataLen: number): void;
  webring_read_stdout_len(procPtr: number): number;
  webring_read_stdout(procPtr: number, outBuf: number, maxLen: number): number;
  webring_read_stderr_len(procPtr: number): number;
  webring_read_stderr(procPtr: number, outBuf: number, maxLen: number): number;
  webring_is_exited(procPtr: number): number;
  webring_exit_code(procPtr: number): number;
  webring_instructions_executed(procPtr: number): bigint;
  webring_destroy(procPtr: number): void;
}

export class RawWebRingModule {
  private instance: WebAssembly.Instance;
  public exports: WebRingWasmExports;

  constructor(instance: WebAssembly.Instance) {
    this.instance = instance;
    this.exports = instance.exports as unknown as WebRingWasmExports;
  }

  /**
   * Instantiate WebRing from bytes or streaming response.
   */
  static async load(source: BufferSource | Response | string): Promise<RawWebRingModule> {
    let instance: WebAssembly.Instance;

    const noop = () => 0;
    const importObject = {
      __wbindgen_placeholder__: new Proxy({}, { get: () => noop }),
      __wbindgen_externref_xform__: new Proxy({}, { get: () => noop }),
      env: {},
    };

    if (typeof source === 'string') {
      // URL in browser
      if (typeof window !== 'undefined' || typeof self !== 'undefined') {
        const response = await fetch(source);
        const result = await WebAssembly.instantiateStreaming(response, importObject);
        instance = result.instance;
      } else {
        // Node / Bun file path
        const fs = await import('node:fs/promises');
        const bytes = await fs.readFile(source);
        const result = await WebAssembly.instantiate(bytes, importObject);
        instance = result.instance;
      }
    } else if (source instanceof Response) {
      const result = await WebAssembly.instantiateStreaming(source, importObject);
      instance = result.instance;
    } else {
      const result = await WebAssembly.instantiate(source, importObject);
      instance = result.instance;
    }

    return new RawWebRingModule(instance);
  }

  /**
   * Allocate memory inside WebAssembly linear memory and copy bytes.
   */
  allocateBytes(bytes: Uint8Array): { ptr: number; size: number } {
    const size = bytes.length;
    const ptr = this.exports.webring_alloc(size);
    const view = new Uint8Array(this.exports.memory.buffer, ptr, size);
    view.set(bytes);
    return { ptr, size };
  }

  /**
   * Free memory inside WebAssembly linear memory.
   */
  deallocate(ptr: number, size: number): void {
    this.exports.webring_dealloc(ptr, size);
  }
}
