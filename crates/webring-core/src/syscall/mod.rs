//! Linux Syscall Dispatcher for WebRing.

pub mod handlers;
pub mod table;

pub use handlers::{handle_syscall, SyscallContext};
pub use table::*;
