//! Virtual File System (VFS) and File Descriptor Table.

use std::collections::HashMap;

pub const AT_FDCWD: i32 = -100;

#[derive(Debug, Clone)]
pub enum FileDescriptor {
    Stdin,
    Stdout,
    Stderr,
    MemoryFile {
        path: String,
        cursor: usize,
        data: Vec<u8>,
        writable: bool,
    },
}

#[derive(Debug, Default)]
pub struct Vfs {
    /// In-memory files: path -> file content
    files: HashMap<String, Vec<u8>>,
    /// Process file descriptors
    fds: HashMap<i32, FileDescriptor>,
    pub next_fd: i32,
    /// Buffers for captured output and pending input
    pub stdout_sink: Vec<u8>,
    pub stderr_sink: Vec<u8>,
    pub stdin_source: Vec<u8>,
    stdin_cursor: usize,
}

impl Vfs {
    pub fn new() -> Self {
        let mut vfs = Self {
            files: HashMap::new(),
            fds: HashMap::new(),
            next_fd: 3,
            stdout_sink: Vec::new(),
            stderr_sink: Vec::new(),
            stdin_source: Vec::new(),
            stdin_cursor: 0,
        };

        // Standard streams
        vfs.fds.insert(0, FileDescriptor::Stdin);
        vfs.fds.insert(1, FileDescriptor::Stdout);
        vfs.fds.insert(2, FileDescriptor::Stderr);

        vfs
    }

    /// Add a virtual file to the filesystem.
    pub fn add_file(&mut self, path: &str, content: Vec<u8>) {
        self.files.insert(path.to_string(), content);
    }

    /// Provide input bytes to stdin.
    pub fn push_stdin(&mut self, bytes: &[u8]) {
        self.stdin_source.extend_from_slice(bytes);
    }

    /// Drain captured stdout bytes.
    pub fn drain_stdout(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stdout_sink)
    }

    /// Drain captured stderr bytes.
    pub fn drain_stderr(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stderr_sink)
    }

    /// Read from a file descriptor. Returns number of bytes read, or negative errno.
    pub fn read(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        match self.fds.get_mut(&fd) {
            Some(FileDescriptor::Stdin) => {
                let available = self.stdin_source.len() - self.stdin_cursor;
                if available == 0 {
                    return 0; // EOF / no data ready
                }
                let to_read = available.min(buf.len());
                buf[..to_read].copy_from_slice(
                    &self.stdin_source[self.stdin_cursor..self.stdin_cursor + to_read],
                );
                self.stdin_cursor += to_read;
                to_read as i64
            }
            Some(FileDescriptor::MemoryFile { cursor, data, .. }) => {
                let available = data.len().saturating_sub(*cursor);
                if available == 0 {
                    return 0;
                }
                let to_read = available.min(buf.len());
                buf[..to_read].copy_from_slice(&data[*cursor..*cursor + to_read]);
                *cursor += to_read;
                to_read as i64
            }
            Some(FileDescriptor::Stdout) | Some(FileDescriptor::Stderr) => -9, // EBADF
            None => -9, // EBADF
        }
    }

    /// Write to a file descriptor. Returns number of bytes written, or negative errno.
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
            Some(FileDescriptor::MemoryFile {
                cursor,
                data: file_data,
                writable,
                ..
            }) => {
                if !*writable {
                    return -9; // EBADF
                }
                let end = *cursor + data.len();
                if end > file_data.len() {
                    file_data.resize(end, 0);
                }
                file_data[*cursor..end].copy_from_slice(data);
                *cursor = end;
                data.len() as i64
            }
            Some(FileDescriptor::Stdin) => -9, // EBADF
            None => -9, // EBADF
        }
    }

    /// Close a file descriptor.
    pub fn close(&mut self, fd: i32) -> i64 {
        if self.fds.remove(&fd).is_some() {
            0
        } else {
            -9 // EBADF
        }
    }
}
