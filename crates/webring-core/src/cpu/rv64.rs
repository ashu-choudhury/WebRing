//! RISC-V 64-bit (RV64I + RV64M) User-Mode CPU Execution Engine.

use super::registers::Registers;
use crate::memory::{MemoryError, VirtualMemory};
use crate::syscall::{handle_syscall, SyscallContext};
use crate::vfs::Vfs;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CpuError {
    #[error("Memory fault: {0}")]
    Memory(#[from] MemoryError),
    #[error("Illegal instruction 0x{0:08x} at PC 0x{1:016x}")]
    IllegalInstruction(u32, u64),
    #[error("Execution limit reached")]
    StepLimitExceeded,
}

pub struct Rv64Cpu {
    pub regs: Registers,
    pub exited: bool,
    pub exit_code: i32,
    pub instructions_executed: u64,
}

impl Default for Rv64Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Rv64Cpu {
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            exited: false,
            exit_code: 0,
            instructions_executed: 0,
        }
    }

    pub fn set_entry(&mut self, entry: u64, sp: u64) {
        self.regs.pc = entry;
        self.regs.set_sp(sp);
    }

    pub fn set_thread_pointer(&mut self, tp: u64) {
        self.regs.set(4, tp);
    }

    /// Execute a single instruction step.
    pub fn step(&mut self, memory: &mut VirtualMemory, vfs: &mut Vfs) -> Result<bool, CpuError> {
        if self.exited {
            return Ok(false);
        }

        let pc = self.regs.pc;
        let half = memory.read_u16(pc)?;
        let (inst, inst_len) = if (half & 0b11) != 0b11 {
            // Compressed 16-bit instruction!
            let uncompressed = match super::rvc::decompress_rvc(half) {
                Some(u) => u,
                None => return Err(CpuError::IllegalInstruction(half as u32, pc)),
            };
            (uncompressed, 2u64)
        } else {
            // Standard 32-bit instruction!
            (memory.read_u32(pc)?, 4u64)
        };

        self.instructions_executed += 1;

        // Decode instruction fields
        let opcode = inst & 0x7F;
        let rd = ((inst >> 7) & 0x1F) as usize;
        let funct3 = (inst >> 12) & 0x7;
        let rs1 = ((inst >> 15) & 0x1F) as usize;
        let rs2 = ((inst >> 20) & 0x1F) as usize;
        let funct7 = (inst >> 25) & 0x7F;

        // Immediate decodings
        let imm_i = (inst as i32 >> 20) as i64;
        let imm_s = (((inst as i32 >> 25) << 5) | (((inst >> 7) & 0x1F) as i32)) as i64;
        let imm_b = (((inst as i32 >> 31) << 12)
            | (((inst >> 7) & 0x1) << 11) as i32
            | (((inst >> 25) & 0x3F) << 5) as i32
            | (((inst >> 8) & 0xF) << 1) as i32) as i64;
        let imm_u = (inst as i32 & !0xFFF) as i64;
        let imm_j = (((inst as i32 >> 31) << 20)
            | (((inst >> 12) & 0xFF) << 12) as i32
            | (((inst >> 20) & 0x1) << 11) as i32
            | (((inst >> 21) & 0x3FF) << 1) as i32) as i64;

        let mut next_pc = pc + inst_len;

        match opcode {
            // LUI (Load Upper Immediate)
            0x37 => {
                self.regs.set(rd, imm_u as u64);
            }

            // AUIPC (Add Upper Immediate to PC)
            0x17 => {
                let val = (pc as i64 + imm_u) as u64;
                self.regs.set(rd, val);
            }

            // JAL (Jump and Link)
            0x6F => {
                self.regs.set(rd, next_pc);
                next_pc = (pc as i64 + imm_j) as u64;
            }

            // JALR (Jump and Link Register)
            0x67 => {
                let target = ((self.regs.get(rs1) as i64 + imm_i) as u64) & !1;
                self.regs.set(rd, next_pc);
                next_pc = target;
            }

            // Branches (BEQ, BNE, BLT, BGE, BLTU, BGEU)
            0x63 => {
                let v1 = self.regs.get(rs1);
                let v2 = self.regs.get(rs2);
                let taken = match funct3 {
                    0b000 => v1 == v2,                             // BEQ
                    0b001 => v1 != v2,                             // BNE
                    0b100 => (v1 as i64) < (v2 as i64),           // BLT
                    0b101 => (v1 as i64) >= (v2 as i64),          // BGE
                    0b110 => v1 < v2,                              // BLTU
                    0b111 => v1 >= v2,                             // BGEU
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                };
                if taken {
                    next_pc = (pc as i64 + imm_b) as u64;
                }
            }

            // Loads (LB, LH, LW, LD, LBU, LHU, LWU)
            0x03 => {
                let addr = (self.regs.get(rs1) as i64 + imm_i) as u64;
                let val = match funct3 {
                    0b000 => (memory.read_u8(addr)? as i8 as i64) as u64,       // LB
                    0b001 => (memory.read_u16(addr)? as i16 as i64) as u64,    // LH
                    0b010 => (memory.read_u32(addr)? as i32 as i64) as u64,    // LW
                    0b011 => memory.read_u64(addr)?,                            // LD
                    0b100 => memory.read_u8(addr)? as u64,                      // LBU
                    0b101 => memory.read_u16(addr)? as u64,                     // LHU
                    0b110 => memory.read_u32(addr)? as u64,                     // LWU
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                };
                self.regs.set(rd, val);
            }

            // LOAD-FP (FLW, FLD)
            0x07 => {
                let addr = (self.regs.get(rs1) as i64 + imm_i) as u64;
                let val = match funct3 {
                    0b010 => memory.read_u32(addr)? as u64,
                    0b011 => memory.read_u64(addr)?,
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                };
                self.regs.set_f(rd, val);
            }

            // Stores (SB, SH, SW, SD)
            0x23 => {
                let addr = (self.regs.get(rs1) as i64 + imm_s) as u64;
                let val = self.regs.get(rs2);
                match funct3 {
                    0b000 => memory.write_u8(addr, val as u8)?,
                    0b001 => memory.write_u16(addr, val as u16)?,
                    0b010 => memory.write_u32(addr, val as u32)?,
                    0b011 => memory.write_u64(addr, val)?,
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                }
            }

            // STORE-FP (FSW, FSD)
            0x27 => {
                let addr = (self.regs.get(rs1) as i64 + imm_s) as u64;
                let val = self.regs.get_f(rs2);
                match funct3 {
                    0b010 => memory.write_u32(addr, val as u32)?,
                    0b011 => memory.write_u64(addr, val)?,
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                }
            }

            // OP-FP (Floating point operations - RV64F & RV64D)
            0x53 => {
                let s1_f32 = f32::from_bits(self.regs.get_f(rs1) as u32);
                let s2_f32 = f32::from_bits(self.regs.get_f(rs2) as u32);
                let s1_f64 = f64::from_bits(self.regs.get_f(rs1));
                let s2_f64 = f64::from_bits(self.regs.get_f(rs2));

                match funct7 {
                    // FADD.S
                    0b0000000 => {
                        let res = s1_f32 + s2_f32;
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FADD.D
                    0b0000001 => {
                        let res = s1_f64 + s2_f64;
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FSUB.S
                    0b0000100 => {
                        let res = s1_f32 - s2_f32;
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FSUB.D
                    0b0000101 => {
                        let res = s1_f64 - s2_f64;
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FMUL.S
                    0b0001000 => {
                        let res = s1_f32 * s2_f32;
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FMUL.D
                    0b0001001 => {
                        let res = s1_f64 * s2_f64;
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FDIV.S
                    0b0001100 => {
                        let res = s1_f32 / s2_f32;
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FDIV.D
                    0b0001101 => {
                        let res = s1_f64 / s2_f64;
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FSQRT.S
                    0b0101100 => {
                        let res = s1_f32.sqrt();
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FSQRT.D
                    0b0101101 => {
                        let res = s1_f64.sqrt();
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FSGNJ.S / FSGNJN.S / FSGNJX.S
                    0b0010000 => {
                        let b1 = self.regs.get_f(rs1) as u32;
                        let b2 = self.regs.get_f(rs2) as u32;
                        let sign = match funct3 {
                            0b000 => b2 & 0x8000_0000,
                            0b001 => (!b2) & 0x8000_0000,
                            0b010 => (b1 ^ b2) & 0x8000_0000,
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        let res = (b1 & 0x7fff_ffff) | sign;
                        self.regs.set_f(rd, res as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FSGNJ.D / FSGNJN.D / FSGNJX.D
                    0b0010001 => {
                        let b1 = self.regs.get_f(rs1);
                        let b2 = self.regs.get_f(rs2);
                        let sign = match funct3 {
                            0b000 => b2 & 0x8000_0000_0000_0000,
                            0b001 => (!b2) & 0x8000_0000_0000_0000,
                            0b010 => (b1 ^ b2) & 0x8000_0000_0000_0000,
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        let res = (b1 & 0x7fff_ffff_ffff_ffff) | sign;
                        self.regs.set_f(rd, res);
                    }
                    // FMIN.S / FMAX.S
                    0b0010100 => {
                        let res = if funct3 == 0b000 { s1_f32.min(s2_f32) } else { s1_f32.max(s2_f32) };
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FMIN.D / FMAX.D
                    0b0010101 => {
                        let res = if funct3 == 0b000 { s1_f64.min(s2_f64) } else { s1_f64.max(s2_f64) };
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FCVT.S.D
                    0b0100000 => {
                        let res = s1_f64 as f32;
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FCVT.D.S
                    0b0100001 => {
                        let res = s1_f32 as f64;
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FEQ.S / FLT.S / FLE.S
                    0b1010000 => {
                        let val = match funct3 {
                            0b010 => if s1_f32 == s2_f32 { 1 } else { 0 },
                            0b001 => if s1_f32 < s2_f32 { 1 } else { 0 },
                            0b000 => if s1_f32 <= s2_f32 { 1 } else { 0 },
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        self.regs.set(rd, val);
                    }
                    // FEQ.D / FLT.D / FLE.D
                    0b1010001 => {
                        let val = match funct3 {
                            0b010 => if s1_f64 == s2_f64 { 1 } else { 0 },
                            0b001 => if s1_f64 < s2_f64 { 1 } else { 0 },
                            0b000 => if s1_f64 <= s2_f64 { 1 } else { 0 },
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        self.regs.set(rd, val);
                    }
                    // FCVT.W.S / FCVT.WU.S / FCVT.L.S / FCVT.LU.S
                    0b1100000 => {
                        let val = match rs2 {
                            0b00000 => (s1_f32 as i32) as i64 as u64,
                            0b00001 => (s1_f32 as u32) as i32 as i64 as u64,
                            0b00010 => (s1_f32 as i64) as u64,
                            0b00011 => s1_f32 as u64,
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        self.regs.set(rd, val);
                    }
                    // FCVT.W.D / FCVT.WU.D / FCVT.L.D / FCVT.LU.D
                    0b1100001 => {
                        let val = match rs2 {
                            0b00000 => (s1_f64 as i32) as i64 as u64,
                            0b00001 => (s1_f64 as u32) as i32 as i64 as u64,
                            0b00010 => (s1_f64 as i64) as u64,
                            0b00011 => s1_f64 as u64,
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        self.regs.set(rd, val);
                    }
                    // FCVT.S.W / FCVT.S.WU / FCVT.S.L / FCVT.S.LU
                    0b1101000 => {
                        let iv = self.regs.get(rs1);
                        let res = match rs2 {
                            0b00000 => (iv as i32) as f32,
                            0b00001 => (iv as u32) as f32,
                            0b00010 => (iv as i64) as f32,
                            0b00011 => iv as f32,
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                    }
                    // FCVT.D.W / FCVT.D.WU / FCVT.D.L / FCVT.D.LU
                    0b1101001 => {
                        let iv = self.regs.get(rs1);
                        let res = match rs2 {
                            0b00000 => (iv as i32) as f64,
                            0b00001 => (iv as u32) as f64,
                            0b00010 => (iv as i64) as f64,
                            0b00011 => iv as f64,
                            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                        };
                        self.regs.set_f(rd, res.to_bits());
                    }
                    // FMV.X.W
                    0b1110000 => self.regs.set(rd, (self.regs.get_f(rs1) as i32) as i64 as u64),
                    // FMV.W.X
                    0b1111000 => self.regs.set_f(rd, (self.regs.get(rs1) & 0xffff_ffff) | 0xffff_ffff_0000_0000),
                    // FMV.X.D
                    0b1110001 => self.regs.set(rd, self.regs.get_f(rs1)),
                    // FMV.D.X
                    0b1111001 => self.regs.set_f(rd, self.regs.get(rs1)),
                    _ => self.regs.set_f(rd, 0),
                }
            }

            // OP-IMM (ADDI, SLTI, SLTIU, XORI, ORI, ANDI, SLLI, SRLI, SRAI)
            0x13 => {
                let v = self.regs.get(rs1);
                let shamt = (inst >> 20) & 0x3F;
                let res = match funct3 {
                    0b000 => (v as i64 + imm_i) as u64,                         // ADDI
                    0b010 => if (v as i64) < imm_i { 1 } else { 0 },           // SLTI
                    0b011 => if v < (imm_i as u64) { 1 } else { 0 },           // SLTIU
                    0b100 => v ^ (imm_i as u64),                                // XORI
                    0b110 => v | (imm_i as u64),                                // ORI
                    0b111 => v & (imm_i as u64),                                // ANDI
                    0b001 => v << shamt,                                        // SLLI
                    0b101 => {
                        if (funct7 & 0x20) != 0 {
                            ((v as i64) >> shamt) as u64                       // SRAI
                        } else {
                            v >> shamt                                          // SRLI
                        }
                    }
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                };
                self.regs.set(rd, res);
            }

            // OP-IMM-32 (ADDIW, SLLIW, SRLIW, SRAIW)
            0x1B => {
                let v = self.regs.get(rs1) as u32;
                let shamt = (inst >> 20) & 0x1F;
                let res = match funct3 {
                    0b000 => (v as i32 + imm_i as i32) as i64 as u64,          // ADDIW
                    0b001 => ((v << shamt) as i32) as i64 as u64,              // SLLIW
                    0b101 => {
                        if (funct7 & 0x20) != 0 {
                            (((v as i32) >> shamt) as i32) as i64 as u64       // SRAIW
                        } else {
                            ((v >> shamt) as i32) as i64 as u64                // SRLIW
                        }
                    }
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                };
                self.regs.set(rd, res);
            }

            // OP (Register-Register 64-bit and RV64M)
            0x33 => {
                let v1 = self.regs.get(rs1);
                let v2 = self.regs.get(rs2);
                let res = if funct7 == 0x01 {
                    // RV64M (Multiply & Divide)
                    match funct3 {
                        0b000 => v1.wrapping_mul(v2),                           // MUL
                        0b001 => ((v1 as i128 * v2 as i128) >> 64) as u64,     // MULH
                        0b010 => ((v1 as i128 * v2 as u128 as i128) >> 64) as u64, // MULHSU
                        0b011 => ((v1 as u128 * v2 as u128) >> 64) as u64,     // MULHU
                        0b100 => { // DIV
                            if v2 == 0 { !0 }
                            else if v1 == (1 << 63) && v2 == !0 { v1 }
                            else { ((v1 as i64) / (v2 as i64)) as u64 }
                        }
                        0b101 => if v2 == 0 { !0 } else { v1 / v2 },           // DIVU
                        0b110 => { // REM
                            if v2 == 0 { v1 }
                            else if v1 == (1 << 63) && v2 == !0 { 0 }
                            else { ((v1 as i64) % (v2 as i64)) as u64 }
                        }
                        0b111 => if v2 == 0 { v1 } else { v1 % v2 },           // REMU
                        _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                    }
                } else {
                    // Standard RV64I ALU
                    match (funct3, funct7) {
                        (0b000, 0x00) => v1.wrapping_add(v2),                   // ADD
                        (0b000, 0x20) => v1.wrapping_sub(v2),                   // SUB
                        (0b001, 0x00) => v1 << (v2 & 0x3F),                     // SLL
                        (0b010, 0x00) => if (v1 as i64) < (v2 as i64) { 1 } else { 0 }, // SLT
                        (0b011, 0x00) => if v1 < v2 { 1 } else { 0 },          // SLTU
                        (0b100, 0x00) => v1 ^ v2,                               // XOR
                        (0b101, 0x00) => v1 >> (v2 & 0x3F),                     // SRL
                        (0b101, 0x20) => ((v1 as i64) >> (v2 & 0x3F)) as u64,  // SRA
                        (0b110, 0x00) => v1 | v2,                               // OR
                        (0b111, 0x00) => v1 & v2,                               // AND
                        _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                    }
                };
                self.regs.set(rd, res);
            }

            // OP-32 (32-bit Register-Register and RV64M 32-bit)
            0x3B => {
                let v1 = self.regs.get(rs1) as u32;
                let v2 = self.regs.get(rs2) as u32;
                let res = if funct7 == 0x01 {
                    // RV64M 32-bit
                    match funct3 {
                        0b000 => (v1 as i32).wrapping_mul(v2 as i32) as i64 as u64, // MULW
                        0b100 => { // DIVW
                            if v2 == 0 { !0 }
                            else if v1 == (1 << 31) && v2 == !0 { v1 as i32 as i64 as u64 }
                            else { ((v1 as i32) / (v2 as i32)) as i64 as u64 }
                        }
                        0b101 => if v2 == 0 { !0 } else { (v1 / v2) as i32 as i64 as u64 }, // DIVUW
                        0b110 => { // REMW
                            if v2 == 0 { v1 as i32 as i64 as u64 }
                            else if v1 == (1 << 31) && v2 == !0 { 0 }
                            else { ((v1 as i32) % (v2 as i32)) as i64 as u64 }
                        }
                        0b111 => if v2 == 0 { v1 as i32 as i64 as u64 } else { (v1 % v2) as i32 as i64 as u64 }, // REMUW
                        _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                    }
                } else {
                    match (funct3, funct7) {
                        (0b000, 0x00) => (v1.wrapping_add(v2) as i32) as i64 as u64, // ADDW
                        (0b000, 0x20) => (v1.wrapping_sub(v2) as i32) as i64 as u64, // SUBW
                        (0b001, 0x00) => ((v1 << (v2 & 0x1F)) as i32) as i64 as u64, // SLLW
                        (0b101, 0x00) => ((v1 >> (v2 & 0x1F)) as i32) as i64 as u64, // SRLW
                        (0b101, 0x20) => (((v1 as i32) >> (v2 & 0x1F)) as i32) as i64 as u64, // SRAW
                        _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                    }
                };
                self.regs.set(rd, res);
            }

            // AMO (Atomic Memory Operations - "A" Extension)
            0x2F => {
                let addr = self.regs.get(rs1);
                let amo_op = (inst >> 27) & 0x1F;
                let is_64 = match funct3 {
                    0b010 => false, // .W (32-bit)
                    0b011 => true,  // .D (64-bit)
                    _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                };

                let v2 = self.regs.get(rs2);

                if is_64 {
                    let old = memory.read_u64(addr)?;
                    let (new_val, rd_val) = match amo_op {
                        0b00010 => (old, old), // LR.D
                        0b00011 => (v2, 0),    // SC.D (success)
                        0b00001 => (v2, old),  // AMOSWAP.D
                        0b00000 => (old.wrapping_add(v2), old), // AMOADD.D
                        0b00100 => (old ^ v2, old), // AMOXOR.D
                        0b01100 => (old & v2, old), // AMOAND.D
                        0b01000 => (old | v2, old), // AMOOR.D
                        0b10000 => (std::cmp::min(old as i64, v2 as i64) as u64, old), // AMOMIN.D
                        0b10100 => (std::cmp::max(old as i64, v2 as i64) as u64, old), // AMOMAX.D
                        0b11000 => (std::cmp::min(old, v2), old), // AMOMINU.D
                        0b11100 => (std::cmp::max(old, v2), old), // AMOMAXU.D
                        _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                    };
                    if amo_op != 0b00010 {
                        memory.write_u64(addr, new_val)?;
                    }
                    self.regs.set(rd, rd_val);
                } else {
                    let old_u32 = memory.read_u32(addr)?;
                    let old = (old_u32 as i32) as i64 as u64; // Sign-extend 32-bit
                    let v2_u32 = v2 as u32;
                    let (new_u32, rd_val) = match amo_op {
                        0b00010 => (old_u32, old), // LR.W
                        0b00011 => (v2_u32, 0),    // SC.W (success)
                        0b00001 => (v2_u32, old),  // AMOSWAP.W
                        0b00000 => (old_u32.wrapping_add(v2_u32), old), // AMOADD.W
                        0b00100 => (old_u32 ^ v2_u32, old), // AMOXOR.W
                        0b01100 => (old_u32 & v2_u32, old), // AMOAND.W
                        0b01000 => (old_u32 | v2_u32, old), // AMOOR.W
                        0b10000 => (std::cmp::min(old_u32 as i32, v2_u32 as i32) as u32, old), // AMOMIN.W
                        0b10100 => (std::cmp::max(old_u32 as i32, v2_u32 as i32) as u32, old), // AMOMAX.W
                        0b11000 => (std::cmp::min(old_u32, v2_u32), old), // AMOMINU.W
                        0b11100 => (std::cmp::max(old_u32, v2_u32), old), // AMOMAXU.W
                        _ => return Err(CpuError::IllegalInstruction(inst, pc)),
                    };
                    if amo_op != 0b00010 {
                        memory.write_u32(addr, new_u32)?;
                    }
                    self.regs.set(rd, rd_val);
                }
            }

            // FENCE
            0x0F => {
                // No-op in user space
            }

            // FMA (FMADD, FMSUB, FNMSUB, FNMADD)
            0x43 | 0x47 | 0x4B | 0x4F => {
                let rs3 = ((inst >> 27) & 0x1F) as usize;
                let funct2 = (inst >> 25) & 0x3;
                if funct2 == 0 {
                    let s1 = f32::from_bits(self.regs.get_f(rs1) as u32);
                    let s2 = f32::from_bits(self.regs.get_f(rs2) as u32);
                    let s3 = f32::from_bits(self.regs.get_f(rs3) as u32);
                    let res = match opcode {
                        0x43 => (s1 * s2) + s3,
                        0x47 => (s1 * s2) - s3,
                        0x4B => -(s1 * s2) + s3,
                        0x4F => -(s1 * s2) - s3,
                        _ => unreachable!(),
                    };
                    self.regs.set_f(rd, res.to_bits() as u64 | 0xffff_ffff_0000_0000);
                } else if funct2 == 1 {
                    let s1 = f64::from_bits(self.regs.get_f(rs1));
                    let s2 = f64::from_bits(self.regs.get_f(rs2));
                    let s3 = f64::from_bits(self.regs.get_f(rs3));
                    let res = match opcode {
                        0x43 => (s1 * s2) + s3,
                        0x47 => (s1 * s2) - s3,
                        0x4B => -(s1 * s2) + s3,
                        0x4F => -(s1 * s2) - s3,
                        _ => unreachable!(),
                    };
                    self.regs.set_f(rd, res.to_bits());
                } else {
                    return Err(CpuError::IllegalInstruction(inst, pc));
                }
            }

            // SYSTEM (ECALL)
            0x73 => {
                if funct3 == 0 && (inst >> 20) == 0 {
                    // ECALL: Dispatch Linux Syscall!
                    handle_syscall(SyscallContext {
                        regs: &mut self.regs,
                        memory,
                        vfs,
                        exited: &mut self.exited,
                        exit_code: &mut self.exit_code,
                    });
                } else {
                    return Err(CpuError::IllegalInstruction(inst, pc));
                }
            }

            _ => return Err(CpuError::IllegalInstruction(inst, pc)),
        }

        self.regs.pc = next_pc;
        Ok(!self.exited)
    }

    /// Run continuously up to a maximum step count or until process exits.
    pub fn run(
        &mut self,
        memory: &mut VirtualMemory,
        vfs: &mut Vfs,
        max_steps: u64,
    ) -> Result<i32, CpuError> {
        let mut steps = 0;
        while !self.exited {
            if steps >= max_steps {
                return Err(CpuError::StepLimitExceeded);
            }
            self.step(memory, vfs)?;
            steps += 1;
        }
        Ok(self.exit_code)
    }
}
