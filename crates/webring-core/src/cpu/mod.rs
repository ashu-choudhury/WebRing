//! CPU Emulation Modules for WebRing.

pub mod registers;
pub mod rv64;

pub use registers::Registers;
pub use rv64::{CpuError, Rv64Cpu};
