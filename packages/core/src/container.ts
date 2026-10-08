import { RawWebRingModule } from './raw.js';
import { WebRingProcess } from './process.js';

export interface ContainerConfig {
  wasmSource: BufferSource | Response | string;
}

export class WebRingContainer {
  private raw: RawWebRingModule;

  constructor(raw: RawWebRingModule) {
    this.raw = raw;
  }

  /**
   * Initialize a new WebRing container engine instance.
   */
  static async init(config: ContainerConfig): Promise<WebRingContainer> {
    const raw = await RawWebRingModule.load(config.wasmSource);
    return new WebRingContainer(raw);
  }

  /**
   * Spawn a new process from an ELF-64 container executable.
   */
  exec(elfBinary: Uint8Array): WebRingProcess {
    const { ptr, size } = this.raw.allocateBytes(elfBinary);
    const procPtr = this.raw.exports.webring_create(ptr, size);
    this.raw.deallocate(ptr, size);

    if (procPtr === 0) {
      throw new Error('Failed to parse or load ELF-64 binary into WebRing container.');
    }

    return new WebRingProcess(this.raw, procPtr);
  }
}
