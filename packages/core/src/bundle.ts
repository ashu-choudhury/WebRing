/**
 * WebRing Container Bundle (.ring) Specification & Utilities.
 * 
 * Format:
 * [0..4]   Magic: 'RING'
 * [4..6]   Version: 0x0001 (uint16 BE)
 * [6..10]  Manifest Length: N (uint32 BE)
 * [10..10+N] Manifest JSON (UTF-8)
 * [10+N..] Rootfs USTAR tarball payload
 */

export interface ContainerManifest {
  name?: string;
  version?: string;
  arch: 'riscv64';
  entrypoint: string;
  cmd?: string[];
  env?: string[];
  workdir?: string;
}

const MAGIC = new Uint8Array([0x52, 0x49, 0x4E, 0x47]); // 'RING'
const VERSION = 1;

export class RingBundle {
  /**
   * Packs a manifest and rootfs tarball into a .ring container bundle.
   */
  static pack(manifest: ContainerManifest, rootfsTar: Uint8Array): Uint8Array {
    const jsonStr = JSON.stringify(manifest, null, 2);
    const jsonBytes = new TextEncoder().encode(jsonStr);

    const totalLen = 4 + 2 + 4 + jsonBytes.length + rootfsTar.length;
    const output = new Uint8Array(totalLen);
    const view = new DataView(output.buffer);

    // Magic
    output.set(MAGIC, 0);

    // Version (uint16 BE)
    view.setUint16(4, VERSION, false);

    // Manifest Length (uint32 BE)
    view.setUint32(6, jsonBytes.length, false);

    // Manifest JSON
    output.set(jsonBytes, 10);

    // Payload (RootFS tar)
    output.set(rootfsTar, 10 + jsonBytes.length);

    return output;
  }

  /**
   * Parses a .ring container bundle.
   */
  static parse(bundleBytes: Uint8Array): {
    manifest: ContainerManifest;
    rootfsTar: Uint8Array;
    extractFile(path: string): Uint8Array | null;
  } {
    if (bundleBytes.length < 10) {
      throw new Error('Invalid WebRing bundle: file too small.');
    }

    // Check magic
    if (
      bundleBytes[0] !== MAGIC[0] ||
      bundleBytes[1] !== MAGIC[1] ||
      bundleBytes[2] !== MAGIC[2] ||
      bundleBytes[3] !== MAGIC[3]
    ) {
      throw new Error('Invalid WebRing bundle: magic header mismatch (expected "RING").');
    }

    const view = new DataView(bundleBytes.buffer, bundleBytes.byteOffset, bundleBytes.byteLength);
    const version = view.getUint16(4, false);
    if (version !== 1) {
      throw new Error(`Unsupported WebRing bundle version: ${version}`);
    }

    const manifestLen = view.getUint32(6, false);
    if (10 + manifestLen > bundleBytes.length) {
      throw new Error('Corrupted WebRing bundle: manifest length exceeds file size.');
    }

    const manifestBytes = bundleBytes.subarray(10, 10 + manifestLen);
    const manifestJson = new TextDecoder().decode(manifestBytes);
    const manifest = JSON.parse(manifestJson) as ContainerManifest;

    const rootfsTar = bundleBytes.subarray(10 + manifestLen);

    // Tar file lookup helper
    const extractFile = (targetPath: string): Uint8Array | null => {
      let norm = targetPath.replace(/^\/+/, '');
      let offset = 0;
      const dec = new TextDecoder();

      while (offset + 512 <= rootfsTar.length) {
        // Check for empty block
        let isZero = true;
        for (let i = 0; i < 512; i++) {
          if (rootfsTar[offset + i] !== 0) {
            isZero = false;
            break;
          }
        }
        if (isZero) break;

        // Read filename
        let nameBytes = rootfsTar.subarray(offset, offset + 100);
        let nullIdx = nameBytes.indexOf(0);
        if (nullIdx === -1) nullIdx = 100;
        let name = dec.decode(nameBytes.subarray(0, nullIdx)).replace(/^\/+/, '');

        // Read file size (octal)
        let sizeBytes = rootfsTar.subarray(offset + 124, offset + 136);
        let sizeStr = dec.decode(sizeBytes).trim().replace(/\0.*$/, '');
        let size = parseInt(sizeStr, 8) || 0;

        let contentStart = offset + 512;
        let contentEnd = contentStart + size;

        const baseName = norm.split('/').pop();
        if (
          name === norm ||
          name === './' + norm ||
          norm.endsWith('/' + name) ||
          name.endsWith('/' + norm) ||
          (baseName && name === baseName)
        ) {
          return rootfsTar.slice(contentStart, contentEnd);
        }

        // Advance to next 512-byte boundary
        offset += 512 + Math.ceil(size / 512) * 512;
      }
      return null;
    };

    return { manifest, rootfsTar, extractFile };
  }
}
