//! ELF Parsing and Loading Module for WebRing.

pub mod header;
pub mod loader;
pub mod program;

pub use header::{ElfError, ElfHeader, EM_RISCV, EM_X86_64};
pub use loader::{ElfLoader, LoadedProcess, LoaderError};
pub use program::{ProgramHeader, PT_LOAD, PT_PHDR};
