//! Syscall implementations for Linux ABI on WebRing.

use super::table::*;
use crate::cpu::registers::Registers;
use crate::memory::VirtualMemory;
use crate::vfs::Vfs;

pub struct SyscallContext<'a> {
    pub regs: &'a mut Registers,
    pub memory: &'a mut VirtualMemory,
    pub vfs: &'a mut Vfs,
    pub exited: &'a mut bool,
    pub exit_code: &'a mut i32,
}

pub fn handle_syscall(ctx: SyscallContext) {
    let nr = ctx.regs.a7();
    let a0 = ctx.regs.a0();
    let a1 = ctx.regs.a1();
    let a2 = ctx.regs.a2();

    let ret: i64 = match nr {
        SYS_WRITE => handle_sys_write(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_WRITEV => handle_sys_writev(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_READ => handle_sys_read(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_CLOSE => ctx.vfs.close(a0 as i32),
        SYS_BRK => ctx.memory.brk(a0) as i64,
        SYS_EXIT | SYS_EXIT_GROUP => {
            *ctx.exited = true;
            *ctx.exit_code = a0 as i32;
            0
        }
        SYS_GETPID | SYS_GETTID => 1000,
        SYS_GETPPID => 1,
        SYS_GETUID | SYS_GETEUID | SYS_GETGID | SYS_GETEGID => 0, // Root in container
        SYS_SET_TID_ADDRESS => 1000,
        SYS_IOCTL => -25, // ENOTTY (Not a typewriter - typical when not a real tty)
        SYS_PRLIMIT64 => 0, // Success stub for resource limits
        _ => {
            log::warn!("Unhandled syscall: nr={}", nr);
            ENOSYS
        }
    };

    ctx.regs.set_a0(ret as u64);
}

fn handle_sys_write(
    memory: &VirtualMemory,
    vfs: &mut Vfs,
    fd: i32,
    buf_ptr: u64,
    count: usize,
) -> i64 {
    if count == 0 {
        return 0;
    }
    let mut buf = vec![0u8; count];
    if memory.read_bytes(buf_ptr, &mut buf).is_err() {
        return EFAULT;
    }
    vfs.write(fd, &buf)
}

fn handle_sys_writev(
    memory: &VirtualMemory,
    vfs: &mut Vfs,
    fd: i32,
    iov_ptr: u64,
    iovcnt: usize,
) -> i64 {
    let mut total_written: i64 = 0;
    for i in 0..iovcnt {
        let entry_addr = iov_ptr + (i as u64 * 16);
        let base = match memory.read_u64(entry_addr) {
            Ok(b) => b,
            Err(_) => return EFAULT,
        };
        let len = match memory.read_u64(entry_addr + 8) {
            Ok(l) => l as usize,
            Err(_) => return EFAULT,
        };

        if len == 0 {
            continue;
        }

        let mut buf = vec![0u8; len];
        if memory.read_bytes(base, &mut buf).is_err() {
            return EFAULT;
        }

        let written = vfs.write(fd, &buf);
        if written < 0 {
            return written;
        }
        total_written += written;
    }
    total_written
}

fn handle_sys_read(
    memory: &mut VirtualMemory,
    vfs: &mut Vfs,
    fd: i32,
    buf_ptr: u64,
    count: usize,
) -> i64 {
    if count == 0 {
        return 0;
    }
    let mut buf = vec![0u8; count];
    let n = vfs.read(fd, &mut buf);
    if n > 0 {
        if memory.write_bytes(buf_ptr, &buf[..n as usize]).is_err() {
            return EFAULT;
        }
    }
    n
}
