//! C-compatible Foreign Function Interface (FFI) for direct WebAssembly consumption.
//! Allows using webring_core.wasm without any external code-generation tools.

use crate::cpu::Rv64Cpu;
use crate::elf::ElfLoader;
use crate::memory::VirtualMemory;
use crate::vfs::Vfs;
use crate::WebRingProcess;

#[unsafe(no_mangle)]
pub extern "C" fn webring_alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_dealloc(ptr: *mut u8, size: usize) {
    if !ptr.is_null() && size > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, 0, size);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_create(
    binary_ptr: *const u8,
    binary_len: usize,
) -> *mut WebRingProcess {
    if binary_ptr.is_null() || binary_len == 0 {
        return std::ptr::null_mut();
    }
    let binary = unsafe { std::slice::from_raw_parts(binary_ptr, binary_len) };
    let mut memory = VirtualMemory::new();
    let args = vec!["/bin/container".to_string()];
    let env = vec![
        "PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string(),
        "TERM=xterm-256color".to_string(),
        "HOME=/root".to_string(),
    ];

    match ElfLoader::load(binary, &mut memory, &args, &env) {
        Ok(loaded) => {
            let mut cpu = Rv64Cpu::new();
            cpu.set_entry(loaded.entry_point, loaded.stack_pointer);
            let vfs = Vfs::new();
            let proc = Box::new(WebRingProcess { cpu, memory, vfs });
            Box::into_raw(proc)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_mount_tar(
    proc_ptr: *mut WebRingProcess,
    tar_ptr: *const u8,
    tar_len: usize,
) -> i32 {
    if proc_ptr.is_null() || tar_ptr.is_null() || tar_len == 0 {
        return -1;
    }
    let proc = unsafe { &mut *proc_ptr };
    let tar_bytes = unsafe { std::slice::from_raw_parts(tar_ptr, tar_len) };
    match proc.vfs.mount_tar(tar_bytes) {
        Ok(count) => count as i32,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_step(proc_ptr: *mut WebRingProcess) -> i32 {
    if proc_ptr.is_null() {
        return -1;
    }
    let proc = unsafe { &mut *proc_ptr };
    match proc.step() {
        Ok(running) => if running { 1 } else { 0 },
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_run(proc_ptr: *mut WebRingProcess, max_steps: u64) -> i32 {
    if proc_ptr.is_null() {
        return -1;
    }
    let proc = unsafe { &mut *proc_ptr };
    match proc.run(max_steps) {
        Ok(code) => code,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_write_stdin(
    proc_ptr: *mut WebRingProcess,
    data_ptr: *const u8,
    data_len: usize,
) {
    if proc_ptr.is_null() || data_ptr.is_null() || data_len == 0 {
        return;
    }
    let proc = unsafe { &mut *proc_ptr };
    let slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
    proc.write_stdin(slice);
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_read_stdout_len(proc_ptr: *mut WebRingProcess) -> usize {
    if proc_ptr.is_null() {
        return 0;
    }
    let proc = unsafe { &mut *proc_ptr };
    proc.vfs.stdout_sink.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_read_stdout(
    proc_ptr: *mut WebRingProcess,
    out_buf: *mut u8,
    max_len: usize,
) -> usize {
    if proc_ptr.is_null() || out_buf.is_null() || max_len == 0 {
        return 0;
    }
    let proc = unsafe { &mut *proc_ptr };
    let bytes = proc.read_stdout();
    let to_copy = bytes.len().min(max_len);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, to_copy);
    }
    to_copy
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_read_stderr_len(proc_ptr: *mut WebRingProcess) -> usize {
    if proc_ptr.is_null() {
        return 0;
    }
    let proc = unsafe { &mut *proc_ptr };
    proc.vfs.stderr_sink.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_read_stderr(
    proc_ptr: *mut WebRingProcess,
    out_buf: *mut u8,
    max_len: usize,
) -> usize {
    if proc_ptr.is_null() || out_buf.is_null() || max_len == 0 {
        return 0;
    }
    let proc = unsafe { &mut *proc_ptr };
    let bytes = proc.read_stderr();
    let to_copy = bytes.len().min(max_len);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, to_copy);
    }
    to_copy
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_is_exited(proc_ptr: *const WebRingProcess) -> i32 {
    if proc_ptr.is_null() {
        return 1;
    }
    let proc = unsafe { &*proc_ptr };
    if proc.is_exited() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_exit_code(proc_ptr: *const WebRingProcess) -> i32 {
    if proc_ptr.is_null() {
        return -1;
    }
    let proc = unsafe { &*proc_ptr };
    proc.exit_code()
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_instructions_executed(proc_ptr: *const WebRingProcess) -> u64 {
    if proc_ptr.is_null() {
        return 0;
    }
    let proc = unsafe { &*proc_ptr };
    proc.instructions_executed()
}

#[unsafe(no_mangle)]
pub extern "C" fn webring_destroy(proc_ptr: *mut WebRingProcess) {
    if !proc_ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(proc_ptr);
        }
    }
}
