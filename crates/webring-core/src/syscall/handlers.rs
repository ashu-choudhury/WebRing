//! Comprehensive Syscall implementations for Linux ABI on WebRing.

use super::table::*;
use crate::cpu::registers::Registers;
use crate::memory::{PagePermissions, VirtualMemory};
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
    let a3 = ctx.regs.a3();
    let a4 = ctx.regs.a4();
    let _a5 = ctx.regs.a5();

    let ret: i64 = match nr {
        SYS_WRITE => handle_sys_write(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_WRITEV => handle_sys_writev(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_READ => handle_sys_read(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_OPENAT => handle_sys_openat(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as i32, a3 as u32),
        SYS_CLOSE => ctx.vfs.close(a0 as i32),
        SYS_LSEEK => ctx.vfs.lseek(a0 as i32, a1 as i64, a2 as i32),
        SYS_GETDENTS64 => handle_sys_getdents64(ctx.memory, ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_NEWFSTATAT => handle_sys_newfstatat(ctx.memory, ctx.vfs, a0 as i32, a1, a2),
        SYS_FSTAT => handle_sys_fstat(ctx.memory, ctx.vfs, a0 as i32, a1),
        SYS_GETCWD => handle_sys_getcwd(ctx.memory, ctx.vfs, a0, a1 as usize),
        SYS_UNAME => handle_sys_uname(ctx.memory, a0),
        SYS_SOCKET => ctx.vfs.socket(a0 as i32, a1 as i32, a2 as i32),
        SYS_CONNECT => handle_sys_connect(ctx.vfs, a0 as i32, a1, a2 as usize),
        SYS_CLOCK_GETTIME => handle_sys_clock_gettime(ctx.memory, a0 as i32, a1),
        SYS_GETTIMEOFDAY => handle_sys_gettimeofday(ctx.memory, a0),
        SYS_GETRANDOM => handle_sys_getrandom(ctx.memory, a0, a1 as usize),
        SYS_MMAP => handle_sys_mmap(ctx.memory, a0, a1, a2 as u32, a3 as u32, a4 as i32),
        SYS_MUNMAP => 0, // Stub: unmap success
        SYS_MPROTECT => 0, // Stub: protect success
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
        SYS_FUTEX => 0, // Futex stub
        SYS_RT_SIGACTION | SYS_RT_SIGPROCMASK => 0, // Signal stubs
        SYS_IOCTL => handle_sys_ioctl(ctx.memory, a0 as i32, a1 as u32, a2),
        SYS_FCNTL => 0, // Success stub for fcntl
        SYS_PRLIMIT64 => 0, // Success stub for resource limits
        _ => {
            log::warn!("Unhandled syscall: nr={}", nr);
            ENOSYS
        }
    };

    ctx.regs.set_a0(ret as u64);
}

fn handle_sys_openat(
    memory: &VirtualMemory,
    vfs: &mut Vfs,
    dirfd: i32,
    pathname_ptr: u64,
    flags: i32,
    mode: u32,
) -> i64 {
    let path = match memory.read_c_string(pathname_ptr) {
        Ok(p) => p,
        Err(_) => return EFAULT,
    };
    vfs.openat(dirfd, &path, flags, mode)
}

fn handle_sys_connect(vfs: &mut Vfs, fd: i32, _sockaddr_ptr: u64, _addrlen: usize) -> i64 {
    // Connects virtual socket to default relay endpoint
    vfs.connect(fd, "127.0.0.1:80")
}

fn handle_sys_clock_gettime(memory: &mut VirtualMemory, _clk_id: i32, tp_ptr: u64) -> i64 {
    // struct timespec { i64 tv_sec; i64 tv_nsec; }
    let mut ts = [0u8; 16];
    let now_secs: i64 = 1775640000; // Simulated epoch
    let now_nsecs: i64 = 500_000_000;
    ts[0..8].copy_from_slice(&now_secs.to_le_bytes());
    ts[8..16].copy_from_slice(&now_nsecs.to_le_bytes());

    if memory.write_bytes(tp_ptr, &ts).is_err() {
        return EFAULT;
    }
    0
}

fn handle_sys_gettimeofday(memory: &mut VirtualMemory, tv_ptr: u64) -> i64 {
    // struct timeval { i64 tv_sec; i64 tv_usec; }
    let mut tv = [0u8; 16];
    let now_secs: i64 = 1775640000;
    let now_usecs: i64 = 500_000;
    tv[0..8].copy_from_slice(&now_secs.to_le_bytes());
    tv[8..16].copy_from_slice(&now_usecs.to_le_bytes());

    if memory.write_bytes(tv_ptr, &tv).is_err() {
        return EFAULT;
    }
    0
}

fn handle_sys_getrandom(memory: &mut VirtualMemory, buf_ptr: u64, buflen: usize) -> i64 {
    if buflen == 0 {
        return 0;
    }
    if buflen > 16 * 1024 * 1024 {
        return EINVAL;
    }
    // Deterministic pseudo-random seed bytes for container stability
    let mut rand_bytes = vec![0u8; buflen];
    let mut state: u64 = 0x853c49e6748fea9b;
    for b in rand_bytes.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *b = (state >> 33) as u8;
    }

    if memory.write_bytes(buf_ptr, &rand_bytes).is_err() {
        return EFAULT;
    }
    buflen as i64
}

fn handle_sys_getdents64(
    memory: &mut VirtualMemory,
    vfs: &mut Vfs,
    fd: i32,
    dirp: u64,
    count: usize,
) -> i64 {
    if count == 0 {
        return 0;
    }
    if count > 16 * 1024 * 1024 {
        return EINVAL;
    }
    let mut buf = vec![0u8; count];
    let n = vfs.getdents64(fd, &mut buf);
    if n > 0 {
        if memory.write_bytes(dirp, &buf[..n as usize]).is_err() {
            return EFAULT;
        }
    }
    n
}

fn handle_sys_newfstatat(
    memory: &mut VirtualMemory,
    vfs: &Vfs,
    dirfd: i32,
    pathname_ptr: u64,
    statbuf_ptr: u64,
) -> i64 {
    let path = match memory.read_c_string(pathname_ptr) {
        Ok(p) => p,
        Err(_) => return EFAULT,
    };
    let resolved = vfs.resolve_path(dirfd, &path);
    let mut buf = [0u8; 128];
    let res = vfs.stat_node(&resolved, &mut buf);
    if res == 0 {
        if memory.write_bytes(statbuf_ptr, &buf).is_err() {
            return EFAULT;
        }
    }
    res
}

fn handle_sys_fstat(
    memory: &mut VirtualMemory,
    vfs: &Vfs,
    fd: i32,
    statbuf_ptr: u64,
) -> i64 {
    let mut buf = [0u8; 128];
    let res = vfs.fstat(fd, &mut buf);
    if res == 0 {
        if memory.write_bytes(statbuf_ptr, &buf).is_err() {
            return EFAULT;
        }
    }
    res
}

fn handle_sys_getcwd(memory: &mut VirtualMemory, vfs: &Vfs, buf_ptr: u64, size: usize) -> i64 {
    let bytes = vfs.cwd.as_bytes();
    if bytes.len() + 1 > size {
        return -34; // -ERANGE
    }
    if memory.write_bytes(buf_ptr, bytes).is_err() {
        return EFAULT;
    }
    if memory.write_u8(buf_ptr + bytes.len() as u64, 0).is_err() {
        return EFAULT;
    }
    buf_ptr as i64
}

fn handle_sys_uname(memory: &mut VirtualMemory, buf_ptr: u64) -> i64 {
    let fields = [
        "Linux",
        "webring",
        "6.1.0-webring",
        "#1 WebRing Container",
        "riscv64",
        "localdomain",
    ];

    let mut offset = buf_ptr;
    for field in fields {
        let mut entry = [0u8; 65];
        let bytes = field.as_bytes();
        let len = bytes.len().min(64);
        entry[..len].copy_from_slice(&bytes[..len]);
        if memory.write_bytes(offset, &entry).is_err() {
            return EFAULT;
        }
        offset += 65;
    }
    0
}

fn handle_sys_mmap(
    memory: &mut VirtualMemory,
    addr_hint: u64,
    length: u64,
    _prot: u32,
    _flags: u32,
    _fd: i32,
) -> i64 {
    if length == 0 {
        return EINVAL;
    }
    let target = if addr_hint != 0 {
        addr_hint
    } else {
        let current = memory.mmap_min_addr;
        memory.mmap_min_addr += (length + 4095) & !4095;
        current
    };

    memory.map_region(target, length, PagePermissions::RW);
    target as i64
}

fn handle_sys_ioctl(memory: &mut VirtualMemory, _fd: i32, cmd: u32, arg: u64) -> i64 {
    if cmd == 0x5413 {
        let mut ws = [0u8; 8];
        ws[0..2].copy_from_slice(&24u16.to_le_bytes()); // 24 rows
        ws[2..4].copy_from_slice(&80u16.to_le_bytes()); // 80 cols
        if memory.write_bytes(arg, &ws).is_err() {
            return EFAULT;
        }
        return 0;
    }
    if cmd == 0x5401 {
        return -25; // -ENOTTY
    }
    0
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
    if count > 16 * 1024 * 1024 {
        return EINVAL;
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
    if count > 16 * 1024 * 1024 {
        return EINVAL;
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
