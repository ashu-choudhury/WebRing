//! Minimal, robust USTAR Tar parser for unpacking container image layers into VFS.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TarError {
    #[error("Tar archive truncated or invalid block")]
    UnexpectedEof,
    #[error("Invalid octal number in tar header: {0}")]
    InvalidOctal(String),
}

#[derive(Debug, Clone)]
pub struct TarEntry {
    pub path: String,
    pub size: usize,
    pub mode: u32,
    pub is_dir: bool,
    pub data: Vec<u8>,
}

pub struct TarArchive;

impl TarArchive {
    /// Unpack tar archive bytes into a list of file and directory entries.
    pub fn parse(bytes: &[u8]) -> Result<Vec<TarEntry>, TarError> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + 512 <= bytes.len() {
            let block = &bytes[offset..offset + 512];

            // Two consecutive zero blocks indicate end of archive
            if block.iter().all(|&b| b == 0) {
                break;
            }

            // Extract file name (bytes 0..100)
            let name_end = block[0..100].iter().position(|&b| b == 0).unwrap_or(100);
            let raw_name = String::from_utf8_lossy(&block[0..name_end]).trim().to_string();

            if raw_name.is_empty() {
                offset += 512;
                continue;
            }

            // Clean path (ensure leading slash, strip trailing slash)
            let is_dir_by_name = raw_name.ends_with('/');
            let mut path = raw_name.trim_end_matches('/').to_string();
            if !path.starts_with('/') {
                path = format!("/{}", path);
            }

            // File mode (bytes 100..108, octal)
            let mode = Self::parse_octal(&block[100..108]).unwrap_or(0o644);

            // File size (bytes 124..136, octal)
            let size = Self::parse_octal(&block[124..136]).unwrap_or(0) as usize;

            // Type flag (byte 156)
            // '0' or '\0' = regular file, '5' = directory, '2' = symlink
            let typeflag = block[156];
            let is_dir = typeflag == b'5' || is_dir_by_name;

            offset += 512; // Advance past header

            // Read data blocks
            let mut data = Vec::new();
            if !is_dir && size > 0 {
                let data_end = offset + size;
                if data_end > bytes.len() {
                    return Err(TarError::UnexpectedEof);
                }
                data.extend_from_slice(&bytes[offset..data_end]);

                // Tar pads data to 512-byte boundaries
                let padded_size = (size + 511) / 512 * 512;
                offset += padded_size;
            }

            entries.push(TarEntry {
                path,
                size,
                mode,
                is_dir,
                data,
            });
        }

        Ok(entries)
    }

    fn parse_octal(bytes: &[u8]) -> Result<u32, TarError> {
        let s = String::from_utf8_lossy(bytes);
        let trimmed = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        if trimmed.is_empty() {
            return Ok(0);
        }
        u32::from_str_radix(trimmed, 8)
            .map_err(|_| TarError::InvalidOctal(trimmed.to_string()))
    }
}
