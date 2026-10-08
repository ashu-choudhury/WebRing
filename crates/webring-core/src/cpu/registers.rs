//! RISC-V 64 General-Purpose Registers and PC.

#[derive(Debug, Clone)]
pub struct Registers {
    /// x0 is always hardwired to 0.
    /// x1 = ra (return address)
    /// x2 = sp (stack pointer)
    /// x3 = gp (global pointer)
    /// x4 = tp (thread pointer)
    /// x5-x7 = t0-t2 (temporaries)
    /// x8 = s0 / fp (saved register / frame pointer)
    /// x9 = s1 (saved register)
    /// x10-x17 = a0-a7 (function arguments / syscall args & return values)
    /// x18-x27 = s2-s11 (saved registers)
    /// x28-x31 = t3-t6 (temporaries)
    x: [u64; 32],
    pub pc: u64,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    pub fn new() -> Self {
        Self {
            x: [0u64; 32],
            pc: 0,
        }
    }

    #[inline(always)]
    pub fn get(&self, reg: usize) -> u64 {
        if reg == 0 || reg >= 32 {
            0
        } else {
            self.x[reg]
        }
    }

    #[inline(always)]
    pub fn set(&mut self, reg: usize, val: u64) {
        if reg != 0 && reg < 32 {
            self.x[reg] = val;
        }
    }

    // Helper accessors for standard ABI registers
    #[inline(always)]
    pub fn ra(&self) -> u64 { self.get(1) }
    #[inline(always)]
    pub fn sp(&self) -> u64 { self.get(2) }
    #[inline(always)]
    pub fn set_sp(&mut self, val: u64) { self.set(2, val); }

    #[inline(always)]
    pub fn a0(&self) -> u64 { self.get(10) }
    #[inline(always)]
    pub fn set_a0(&mut self, val: u64) { self.set(10, val); }

    #[inline(always)]
    pub fn a1(&self) -> u64 { self.get(11) }
    #[inline(always)]
    pub fn a2(&self) -> u64 { self.get(12) }
    #[inline(always)]
    pub fn a3(&self) -> u64 { self.get(13) }
    #[inline(always)]
    pub fn a4(&self) -> u64 { self.get(14) }
    #[inline(always)]
    pub fn a5(&self) -> u64 { self.get(15) }
    #[inline(always)]
    pub fn a6(&self) -> u64 { self.get(16) }
    #[inline(always)]
    pub fn a7(&self) -> u64 { self.get(17) } // Syscall ID in Linux RISC-V ABI
}
