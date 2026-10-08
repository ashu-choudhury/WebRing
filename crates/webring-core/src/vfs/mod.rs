//! Virtual File System (VFS) with Container Layer (Tar) mounting and directory traversal.

pub mod tar;

use std::collections::HashMap;
use self::tar::{TarArchive, TarError};

pub const AT_FDCWD: i32 = -100;

// Linux File Types for stat / getdents
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFCHR: u32 = 0o020000;

pub const DT_DIR: u8 = 4;
pub const DT_REG: u8 = 8;
pub const DT_CHR: u8 = 2;

#[derive(Debug, Clone)]
pub struct VfsNode {
    pub is_dir: bool,
    pub data: Vec<u8>,
    pub mode: u32,
    pub inode: u64,
}

#[derive(Debug, Clone)]
pub enum FileDescriptor {
    Stdin,
    Stdout,
    Stderr,
    File {
        path: String,
        cursor: usize,
        writable: bool,
    },
    Directory {
        path: String,
        children: Vec<String>,
        child_index: usize,
    },
}

#[derive(Debug)]
pub struct Vfs {
    /// In-memory files and directories: path -> node
    nodes: HashMap<String, VfsNode>,
    /// Process file descriptors
    fds: HashMap<i32, FileDescriptor>,
    pub next_fd: i32,
    next_inode: u64,
    pub cwd: String,

    /// Captured streams
    pub stdout_sink: Vec<u8>,
    pub stderr_sink: Vec<u8>,
    pub stdin_source: Vec<u8>,
    stdin_cursor: usize,
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs {
    pub fn new() -> Self {
        let mut vfs = Self {
            nodes: HashMap::new(),
            fds: HashMap::new(),
            next_fd: 3,
            next_inode: 100,
            cwd: "/".to_string(),
            stdout_sink: Vec::new(),
            stderr_sink: Vec::new(),
            stdin_source: Vec::new(),
            stdin_cursor: 0,
        };

        // Standard streams
        vfs.fds.insert(0, FileDescriptor::Stdin);
        vfs.fds.insert(1, FileDescriptor::Stdout);
        vfs.fds.insert(2, FileDescriptor::Stderr);

        // Standard root and virtual directories
        vfs.add_directory("/");
        vfs.add_directory("/bin");
        vfs.add_directory("/etc");
        vfs.add_directory("/proc");
        vfs.add_directory("/dev");
        vfs.add_directory("/tmp");

        // Standard Linux container files
        vfs.add_file(
            "/etc/os-release",
            b"NAME=\"WebRing Container Linux\"\nID=webring\nVERSION_ID=\"1.0\"\nPRETTY_NAME=\"WebRing Linux 1.0\"\n".to_vec(),
        );
        vfs.add_file("/etc/hostname", b"webring\n".to_vec());
        vfs.add_file(
            "/proc/version",
            b"Linux version 6.1.0-webring (gcc version 12.2.0) #1 WebRing Container\n".to_vec(),
        );

        vfs
    }

    /// Mount an OCI / Docker rootfs tarball directly into the VFS.
    pub fn mount_tar(&mut self, tar_bytes: &[u8]) -> Result<usize, TarError> {
        let entries = TarArchive::parse(tar_bytes)?;
        let count = entries.len();
        for entry in entries {
            if entry.is_dir {
                self.add_directory(&entry.path);
            } else {
                self.add_file_with_mode(&entry.path, entry.data, entry.mode);
            }
        }
        Ok(count)
    }

    pub fn add_directory(&mut self, path: &str) {
        let norm = self.normalize_path(path);
        if let Some(parent) = self.parent_dir(&norm) {
            self.ensure_dir_exists(&parent);
        }
        let inode = self.alloc_inode();
        self.nodes.insert(
            norm,
            VfsNode {
                is_dir: true,
                data: Vec::new(),
                mode: S_IFDIR | 0o755,
                inode,
            },
        );
    }

    pub fn add_file(&mut self, path: &str, content: Vec<u8>) {
        self.add_file_with_mode(path, content, 0o644);
    }

    pub fn add_file_with_mode(&mut self, path: &str, content: Vec<u8>, mode: u32) {
        let norm = self.normalize_path(path);
        // Ensure parent directories exist
        if let Some(parent) = self.parent_dir(&norm) {
            self.ensure_dir_exists(&parent);
        }

        let inode = self.alloc_inode();
        self.nodes.insert(
            norm,
            VfsNode {
                is_dir: false,
                data: content,
                mode: S_IFREG | (mode & 0o777),
                inode,
            },
        );
    }

    pub fn get_file(&self, path: &str) -> Option<&[u8]> {
        let norm = self.normalize_path(path);
        self.nodes.get(&norm).and_then(|node| {
            if !node.is_dir {
                Some(node.data.as_slice())
            } else {
                None
            }
        })
    }

    pub fn openat(&mut self, dirfd: i32, path: &str, flags: i32, _mode: u32) -> i64 {
        let resolved = self.resolve_path(dirfd, path);

        if let Some(node) = self.nodes.get(&resolved) {
            let fd = self.next_fd;
            self.next_fd += 1;

            if node.is_dir {
                // Find children
                let prefix = if resolved == "/" {
                    "/".to_string()
                } else {
                    format!("{}/", resolved)
                };

                let mut children = vec![".".to_string(), "..".to_string()];
                for p in self.nodes.keys() {
                    if p != &resolved && p.starts_with(&prefix) {
                        let rel = &p[prefix.len()..];
                        if !rel.contains('/') && !rel.is_empty() {
                            children.push(rel.to_string());
                        }
                    }
                }
                children.sort();
                children.dedup();

                self.fds.insert(
                    fd,
                    FileDescriptor::Directory {
                        path: resolved,
                        children,
                        child_index: 0,
                    },
                );
                fd as i64
            } else {
                let writable = (flags & 0o3) != 0; // O_WRONLY or O_RDWR
                self.fds.insert(
                    fd,
                    FileDescriptor::File {
                        path: resolved,
                        cursor: 0,
                        writable,
                    },
                );
                fd as i64
            }
        } else {
            // O_CREAT flag is 0o100 (64)
            if (flags & 64) != 0 {
                self.add_file(&resolved, Vec::new());
                let fd = self.next_fd;
                self.next_fd += 1;
                self.fds.insert(
                    fd,
                    FileDescriptor::File {
                        path: resolved,
                        cursor: 0,
                        writable: true,
                    },
                );
                fd as i64
            } else {
                -2 // -ENOENT
            }
        }
    }

    pub fn read(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        match self.fds.get_mut(&fd) {
            Some(FileDescriptor::Stdin) => {
                let available = self.stdin_source.len() - self.stdin_cursor;
                if available == 0 {
                    return 0;
                }
                let to_read = available.min(buf.len());
                buf[..to_read].copy_from_slice(
                    &self.stdin_source[self.stdin_cursor..self.stdin_cursor + to_read],
                );
                self.stdin_cursor += to_read;
                to_read as i64
            }
            Some(FileDescriptor::File { path, cursor, .. }) => {
                if let Some(node) = self.nodes.get(path) {
                    let available = node.data.len().saturating_sub(*cursor);
                    if available == 0 {
                        return 0;
                    }
                    let to_read = available.min(buf.len());
                    buf[..to_read].copy_from_slice(&node.data[*cursor..*cursor + to_read]);
                    *cursor += to_read;
                    to_read as i64
                } else {
                    -9 // -EBADF
                }
            }
            Some(FileDescriptor::Directory { .. }) => -21, // -EISDIR
            _ => -9,                                       // -EBADF
        }
    }

    pub fn write(&mut self, fd: i32, data: &[u8]) -> i64 {
        match self.fds.get_mut(&fd) {
            Some(FileDescriptor::Stdout) => {
                self.stdout_sink.extend_from_slice(data);
                data.len() as i64
            }
            Some(FileDescriptor::Stderr) => {
                self.stderr_sink.extend_from_slice(data);
                data.len() as i64
            }
            Some(FileDescriptor::File {
                path,
                cursor,
                writable,
            }) => {
                if !*writable {
                    return -9; // -EBADF
                }
                if let Some(node) = self.nodes.get_mut(path) {
                    let end = *cursor + data.len();
                    if end > node.data.len() {
                        node.data.resize(end, 0);
                    }
                    node.data[*cursor..end].copy_from_slice(data);
                    *cursor = end;
                    data.len() as i64
                } else {
                    -9 // -EBADF
                }
            }
            _ => -9,
        }
    }

    pub fn close(&mut self, fd: i32) -> i64 {
        if self.fds.remove(&fd).is_some() {
            0
        } else {
            -9 // -EBADF
        }
    }

    /// Read directory entries into a buffer conforming to `struct linux_dirent64`.
    pub fn getdents64(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        let (path, children, child_index) = match self.fds.get_mut(&fd) {
            Some(FileDescriptor::Directory {
                path,
                children,
                child_index,
            }) => (path.clone(), children.clone(), child_index),
            _ => return -9, // -EBADF / not a directory
        };

        let mut offset = 0;
        let mut idx = *child_index;

        while idx < children.len() {
            let name = &children[idx];
            let child_path = if name == "." || name == ".." {
                path.clone()
            } else if path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", path, name)
            };

            let (d_type, inode) = if let Some(node) = self.nodes.get(&child_path) {
                (if node.is_dir { DT_DIR } else { DT_REG }, node.inode)
            } else {
                (DT_REG, self.alloc_inode())
            };

            // Calculate dirent64 size:
            // 8 (ino) + 8 (off) + 2 (reclen) + 1 (type) + name_len + 1 (null)
            // Padded to 8-byte boundary
            let unpadded = 19 + name.len() + 1;
            let reclen = ((unpadded + 7) / 8 * 8) as u16;

            if offset + reclen as usize > buf.len() {
                if offset == 0 {
                    return -22; // -EINVAL: buffer too small for a single entry
                }
                break;
            }

            // Write fields
            let entry_buf = &mut buf[offset..offset + reclen as usize];
            entry_buf.fill(0);

            // d_ino (u64)
            entry_buf[0..8].copy_from_slice(&inode.to_le_bytes());
            // d_off (i64)
            entry_buf[8..16].copy_from_slice(&((idx + 1) as i64).to_le_bytes());
            // d_reclen (u16)
            entry_buf[16..18].copy_from_slice(&reclen.to_le_bytes());
            // d_type (u8)
            entry_buf[18] = d_type;
            // d_name (null-terminated string)
            entry_buf[19..19 + name.len()].copy_from_slice(name.as_bytes());
            entry_buf[19 + name.len()] = 0;

            offset += reclen as usize;
            idx += 1;
        }

        if let Some(FileDescriptor::Directory { child_index, .. }) = self.fds.get_mut(&fd) {
            *child_index = idx;
        }

        offset as i64
    }

    /// Fill Linux 64-bit `struct stat`.
    pub fn stat_node(&self, path: &str, buf: &mut [u8]) -> i64 {
        let norm = self.normalize_path(path);
        let node = match self.nodes.get(&norm) {
            Some(n) => n,
            None => return -2, // -ENOENT
        };

        if buf.len() < 128 {
            return -14; // -EFAULT
        }

        buf.fill(0);
        // st_dev (u64 at 0)
        buf[0..8].copy_from_slice(&1u64.to_le_bytes());
        // st_ino (u64 at 8)
        buf[8..16].copy_from_slice(&node.inode.to_le_bytes());
        // st_mode (u32 at 16)
        buf[16..20].copy_from_slice(&node.mode.to_le_bytes());
        // st_nlink (u32 at 20)
        buf[20..24].copy_from_slice(&1u32.to_le_bytes());
        // st_uid (u32 at 24) = 0
        // st_gid (u32 at 28) = 0
        // st_size (i64 at 48)
        let size = node.data.len() as i64;
        buf[48..56].copy_from_slice(&size.to_le_bytes());
        // st_blksize (i32 at 56) = 4096
        buf[56..60].copy_from_slice(&4096i32.to_le_bytes());
        // st_blocks (i64 at 64)
        let blocks = (size + 511) / 512;
        buf[64..72].copy_from_slice(&blocks.to_le_bytes());

        0 // Success
    }

    pub fn fstat(&self, fd: i32, buf: &mut [u8]) -> i64 {
        match self.fds.get(&fd) {
            Some(FileDescriptor::Stdin)
            | Some(FileDescriptor::Stdout)
            | Some(FileDescriptor::Stderr) => {
                if buf.len() < 128 {
                    return -14;
                }
                buf.fill(0);
                buf[16..20].copy_from_slice(&(S_IFCHR | 0o666).to_le_bytes()); // Character device
                0
            }
            Some(FileDescriptor::File { path, .. })
            | Some(FileDescriptor::Directory { path, .. }) => self.stat_node(path, buf),
            None => -9, // -EBADF
        }
    }

    pub fn resolve_path(&self, dirfd: i32, path: &str) -> String {
        if path.starts_with('/') {
            self.normalize_path(path)
        } else if dirfd == AT_FDCWD {
            self.normalize_path(&format!("{}/{}", self.cwd, path))
        } else if let Some(FileDescriptor::Directory { path: dir_path, .. }) = self.fds.get(&dirfd)
        {
            self.normalize_path(&format!("{}/{}", dir_path, path))
        } else {
            self.normalize_path(&format!("{}/{}", self.cwd, path))
        }
    }

    pub fn normalize_path(&self, path: &str) -> String {
        let mut segments = Vec::new();
        for seg in path.split('/') {
            if seg.is_empty() || seg == "." {
                continue;
            } else if seg == ".." {
                segments.pop();
            } else {
                segments.push(seg);
            }
        }
        if segments.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", segments.join("/"))
        }
    }

    fn parent_dir(&self, path: &str) -> Option<String> {
        let norm = self.normalize_path(path);
        if norm == "/" {
            None
        } else {
            let last_slash = norm.rfind('/')?;
            if last_slash == 0 {
                Some("/".to_string())
            } else {
                Some(norm[..last_slash].to_string())
            }
        }
    }

    fn ensure_dir_exists(&mut self, path: &str) {
        if !self.nodes.contains_key(path) {
            self.add_directory(path);
        }
    }

    fn alloc_inode(&mut self) -> u64 {
        let ino = self.next_inode;
        self.next_inode += 1;
        ino
    }

    pub fn push_stdin(&mut self, bytes: &[u8]) {
        self.stdin_source.extend_from_slice(bytes);
    }

    pub fn drain_stdout(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stdout_sink)
    }

    pub fn drain_stderr(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stderr_sink)
    }
}
