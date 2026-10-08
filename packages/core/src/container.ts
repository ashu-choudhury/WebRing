import { RawWebRingModule } from './raw.js';
import { WebRingProcess } from './process.js';
import { RingBundle, type ContainerManifest } from './bundle.js';

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
   * Spawn a new process from an ELF-64 container executable with optional command line.
   */
  exec(elfBinary: Uint8Array, cmdLine?: string): WebRingProcess {
    const { ptr: binPtr, size: binSize } = this.raw.allocateBytes(elfBinary);
    let procPtr = 0;

    if (cmdLine && cmdLine.trim().length > 0) {
      const cmdBytes = new TextEncoder().encode(cmdLine);
      const { ptr: cmdPtr, size: cmdSize } = this.raw.allocateBytes(cmdBytes);
      procPtr = this.raw.exports.webring_create_args(binPtr, binSize, cmdPtr, cmdSize);
      this.raw.deallocate(cmdPtr, cmdSize);
    } else {
      procPtr = this.raw.exports.webring_create(binPtr, binSize);
    }

    this.raw.deallocate(binPtr, binSize);

    if (procPtr === 0) {
      throw new Error('Failed to parse or load ELF-64 binary into WebRing container.');
    }

    return new WebRingProcess(this.raw, procPtr);
  }

  /**
   * Load and boot a self-contained WebRing container bundle (.ring).
   */
  execBundle(bundleBytes: Uint8Array, overrideCmd?: string): {
    process: WebRingProcess;
    manifest: ContainerManifest;
  } {
    const { manifest, rootfsTar, extractFile } = RingBundle.parse(bundleBytes);

    let elfBytes = extractFile(manifest.entrypoint);
    if (!elfBytes) {
      // Fallback search common locations
      const fallbacks = ['bin/busybox', 'bin/sh', 'usr/bin/busybox', 'entrypoint'];
      for (const fb of fallbacks) {
        elfBytes = extractFile(fb);
        if (elfBytes) break;
      }
    }

    if (!elfBytes) {
      throw new Error(`Container entrypoint not found in bundle: ${manifest.entrypoint}`);
    }

    const cmdToRun = overrideCmd || (manifest.cmd ? manifest.cmd.join(' ') : manifest.entrypoint);
    const proc = this.exec(elfBytes, cmdToRun);

    // Automatically mount the bundle's root filesystem
    if (rootfsTar.length > 0) {
      proc.mountTar(rootfsTar);
    }

    return { process: proc, manifest };
  }
}
