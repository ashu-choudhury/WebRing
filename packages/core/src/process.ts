import { RawWebRingModule } from './raw.js';

export class WebRingProcess {
  private raw: RawWebRingModule;
  private procPtr: number;
  private destroyed = false;

  constructor(raw: RawWebRingModule, procPtr: number) {
    this.raw = raw;
    this.procPtr = procPtr;
  }

  /**
   * Mount a container rootfs tarball (.tar) into the process's Virtual Filesystem (VFS).
   * Unpacks all files and directories. Returns number of mounted entries.
   */
  mountTar(tarBytes: Uint8Array): number {
    this.ensureAlive();
    const { ptr, size } = this.raw.allocateBytes(tarBytes);
    const count = this.raw.exports.webring_mount_tar(this.procPtr, ptr, size);
    this.raw.deallocate(ptr, size);
    if (count < 0) {
      throw new Error('Failed to unpack container tar layer into VFS');
    }
    return count;
  }

  /**
   * Write data into stdin of the container process.
   */
  write(data: Uint8Array | string): void {
    this.ensureAlive();
    const bytes = typeof data === 'string' ? new TextEncoder().encode(data) : data;
    if (bytes.length === 0) return;

    const { ptr, size } = this.raw.allocateBytes(bytes);
    this.raw.exports.webring_write_stdin(this.procPtr, ptr, size);
    this.raw.deallocate(ptr, size);
  }

  /**
   * Read all buffered stdout bytes.
   */
  readStdout(): Uint8Array {
    this.ensureAlive();
    const len = this.raw.exports.webring_read_stdout_len(this.procPtr);
    if (len === 0) return new Uint8Array(0);

    const outPtr = this.raw.exports.webring_alloc(len);
    const readCount = this.raw.exports.webring_read_stdout(this.procPtr, outPtr, len);
    const view = new Uint8Array(this.raw.exports.memory.buffer, outPtr, readCount);
    const result = new Uint8Array(view); // Make a copy
    this.raw.deallocate(outPtr, len);
    return result;
  }

  /**
   * Read buffered stdout decoded as UTF-8 string.
   */
  readStdoutText(): string {
    const bytes = this.readStdout();
    return new TextDecoder().decode(bytes);
  }

  /**
   * Step a single instruction. Returns true if the process is still running.
   */
  step(): boolean {
    this.ensureAlive();
    const status = this.raw.exports.webring_step(this.procPtr);
    return status === 1;
  }

  /**
   * Run the process up to `maxSteps` instructions or until it exits.
   * Returns the process exit code.
   */
  run(maxSteps = 10_000_000): number {
    this.ensureAlive();
    return this.raw.exports.webring_run(this.procPtr, BigInt(maxSteps));
  }

  /**
   * Check if process has terminated.
   */
  isExited(): boolean {
    this.ensureAlive();
    return this.raw.exports.webring_is_exited(this.procPtr) === 1;
  }

  /**
   * Get the exit code of the terminated process.
   */
  exitCode(): number {
    this.ensureAlive();
    return this.raw.exports.webring_exit_code(this.procPtr);
  }

  /**
   * Total instructions executed so far.
   */
  instructionsExecuted(): number {
    this.ensureAlive();
    return Number(this.raw.exports.webring_instructions_executed(this.procPtr));
  }

  /**
   * Release resources.
   */
  destroy(): void {
    if (!this.destroyed && this.procPtr !== 0) {
      this.raw.exports.webring_destroy(this.procPtr);
      this.procPtr = 0;
      this.destroyed = true;
    }
  }

  private ensureAlive(): void {
    if (this.destroyed || this.procPtr === 0) {
      throw new Error('WebRingProcess has been destroyed or terminated.');
    }
  }
}
