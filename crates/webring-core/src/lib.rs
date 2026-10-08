//! WebRing: Headless User-Mode Container & Linux ABI Runtime for WebAssembly.

pub mod cpu;
pub mod elf;
pub mod ffi;
pub mod memory;
pub mod syscall;
pub mod vfs;

use cpu::Rv64Cpu;
use elf::ElfLoader;
use memory::VirtualMemory;
use vfs::Vfs;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn webring_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// A headless Linux user-space container process running inside WebRing.
#[wasm_bindgen]
pub struct WebRingProcess {
    pub(crate) cpu: Rv64Cpu,
    pub(crate) memory: VirtualMemory,
    pub(crate) vfs: Vfs,
}

#[wasm_bindgen]
impl WebRingProcess {
    /// Load an ELF binary and initialize the process stack with arguments and environment.
    #[wasm_bindgen(constructor)]
    pub fn new(binary: &[u8], args: Vec<String>, env: Vec<String>) -> Result<WebRingProcess, JsError> {
        let mut memory = VirtualMemory::new();
        let loaded = ElfLoader::load(binary, &mut memory, &args, &env)
            .map_err(|e| JsError::new(&format!("ELF load error: {}", e)))?;

        let mut cpu = Rv64Cpu::new();
        cpu.set_entry(loaded.entry_point, loaded.stack_pointer);

        let vfs = Vfs::new();

        Ok(WebRingProcess { cpu, memory, vfs })
    }

    /// Step execution by a single instruction. Returns true if the process is still running.
    pub fn step(&mut self) -> Result<bool, JsError> {
        self.cpu
            .step(&mut self.memory, &mut self.vfs)
            .map_err(|e| JsError::new(&format!("Execution fault: {}", e)))
    }

    /// Run the process up to `max_steps` instructions or until it terminates.
    /// Returns the process exit code.
    pub fn run(&mut self, max_steps: u64) -> Result<i32, JsError> {
        self.cpu
            .run(&mut self.memory, &mut self.vfs, max_steps)
            .map_err(|e| JsError::new(&format!("Execution fault: {}", e)))
    }

    /// Write bytes into the standard input (stdin) stream of the process.
    pub fn write_stdin(&mut self, data: &[u8]) {
        self.vfs.push_stdin(data);
    }

    /// Read and drain all buffered bytes from stdout.
    pub fn read_stdout(&mut self) -> Vec<u8> {
        self.vfs.drain_stdout()
    }

    /// Read and drain all buffered bytes from stderr.
    pub fn read_stderr(&mut self) -> Vec<u8> {
        self.vfs.drain_stderr()
    }

    /// Returns true if the process has exited.
    pub fn is_exited(&self) -> bool {
        self.cpu.exited
    }

    /// Returns the exit code of the process.
    pub fn exit_code(&self) -> i32 {
        self.cpu.exit_code
    }

    /// Returns the total number of instructions executed so far.
    pub fn instructions_executed(&self) -> u64 {
        self.cpu.instructions_executed
    }

    /// Returns the current program counter (PC).
    pub fn pc(&self) -> u64 {
        self.cpu.regs.pc
    }
}
