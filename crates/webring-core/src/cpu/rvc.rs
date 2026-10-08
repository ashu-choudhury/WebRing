//! RISC-V Compressed (RVC / "C" Extension) Instruction Decompressor.
//! Decodes 16-bit compressed instructions into standard 32-bit RISC-V instructions.

pub fn decompress_rvc(inst16: u16) -> Option<u32> {
    let inst = inst16 as u32;
    let op = inst & 0b11;
    let funct3 = (inst >> 13) & 0b111;

    // Helper to decode standard RVC compressed 3-bit register index (x8 - x15)
    let cr = |reg: u32| -> u32 { (reg & 0b111) + 8 };

    match op {
        // Quadrant 00
        0b00 => match funct3 {
            // C.ADDI4SPN: addi rd', x2, nzuimm
            0b000 => {
                let nzuimm = (((inst >> 6) & 1) << 2)
                    | (((inst >> 5) & 1) << 3)
                    | (((inst >> 11) & 0b11) << 4)
                    | (((inst >> 7) & 0b1111) << 6);
                if nzuimm == 0 {
                    return None;
                }
                let rd_prime = cr(inst >> 2);
                Some(0x13 | (rd_prime << 7) | (2 << 15) | (nzuimm << 20))
            }

            // C.FLD: fld rd', offset(rs1')
            0b001 => {
                let offset = (((inst >> 10) & 0b111) << 3)
                    | (((inst >> 5) & 0b11) << 6);
                let rd_prime = cr(inst >> 2);
                let rs1_prime = cr(inst >> 7);
                Some(0x07 | (rd_prime << 7) | (0b011 << 12) | (rs1_prime << 15) | (offset << 20))
            }

            // C.LW: lw rd', offset(rs1')
            0b010 => {
                let offset = (((inst >> 6) & 1) << 2)
                    | (((inst >> 10) & 0b111) << 3)
                    | (((inst >> 5) & 1) << 6);
                let rd_prime = cr(inst >> 2);
                let rs1_prime = cr(inst >> 7);
                Some(0x03 | (rd_prime << 7) | (0b010 << 12) | (rs1_prime << 15) | (offset << 20))
            }

            // C.LD: ld rd', offset(rs1')
            0b011 => {
                let offset = (((inst >> 10) & 0b111) << 3)
                    | (((inst >> 5) & 0b11) << 6);
                let rd_prime = cr(inst >> 2);
                let rs1_prime = cr(inst >> 7);
                Some(0x03 | (rd_prime << 7) | (0b011 << 12) | (rs1_prime << 15) | (offset << 20))
            }

            // C.FSD: fsd rs2', offset(rs1')
            0b101 => {
                let offset = (((inst >> 10) & 0b111) << 3)
                    | (((inst >> 5) & 0b11) << 6);
                let rs2_prime = cr(inst >> 2);
                let rs1_prime = cr(inst >> 7);
                let imm4_0 = offset & 0x1F;
                let imm11_5 = (offset >> 5) & 0x7F;
                Some(0x27 | (imm4_0 << 7) | (0b011 << 12) | (rs1_prime << 15) | (rs2_prime << 20) | (imm11_5 << 25))
            }

            // C.SW: sw rs2', offset(rs1')
            0b110 => {
                let offset = (((inst >> 6) & 1) << 2)
                    | (((inst >> 10) & 0b111) << 3)
                    | (((inst >> 5) & 1) << 6);
                let rs2_prime = cr(inst >> 2);
                let rs1_prime = cr(inst >> 7);
                let imm4_0 = offset & 0x1F;
                let imm11_5 = (offset >> 5) & 0x7F;
                Some(0x23 | (imm4_0 << 7) | (0b010 << 12) | (rs1_prime << 15) | (rs2_prime << 20) | (imm11_5 << 25))
            }

            // C.SD: sd rs2', offset(rs1')
            0b111 => {
                let offset = (((inst >> 10) & 0b111) << 3)
                    | (((inst >> 5) & 0b11) << 6);
                let rs2_prime = cr(inst >> 2);
                let rs1_prime = cr(inst >> 7);
                let imm4_0 = offset & 0x1F;
                let imm11_5 = (offset >> 5) & 0x7F;
                Some(0x23 | (imm4_0 << 7) | (0b011 << 12) | (rs1_prime << 15) | (rs2_prime << 20) | (imm11_5 << 25))
            }

            _ => None,
        },

        // Quadrant 01
        0b01 => match funct3 {
            // C.NOP / C.ADDI: addi rd, rd, nzimm
            0b000 => {
                let rd = (inst >> 7) & 0x1F;
                let imm = (((inst >> 12) & 1) << 5) | ((inst >> 2) & 0x1F);
                let imm_sign = (((imm as i32) << 26) >> 26) as u32;
                Some(0x13 | (rd << 7) | (rd << 15) | ((imm_sign & 0xFFF) << 20))
            }

            // C.ADDIW: addiw rd, rd, imm
            0b001 => {
                let rd = (inst >> 7) & 0x1F;
                if rd == 0 {
                    return None;
                }
                let imm = (((inst >> 12) & 1) << 5) | ((inst >> 2) & 0x1F);
                let imm_sign = (((imm as i32) << 26) >> 26) as u32;
                Some(0x1B | (rd << 7) | (rd << 15) | ((imm_sign & 0xFFF) << 20))
            }

            // C.LI: addi rd, x0, imm
            0b010 => {
                let rd = (inst >> 7) & 0x1F;
                if rd == 0 {
                    return None;
                }
                let imm = (((inst >> 12) & 1) << 5) | ((inst >> 2) & 0x1F);
                let imm_sign = (((imm as i32) << 26) >> 26) as u32;
                Some(0x13 | (rd << 7) | (0 << 15) | ((imm_sign & 0xFFF) << 20))
            }

            // C.ADDI16SP / C.LUI
            0b011 => {
                let rd = (inst >> 7) & 0x1F;
                if rd == 2 {
                    // C.ADDI16SP: addi x2, x2, nzimm
                    let imm = (((inst >> 12) & 1) << 9)
                        | (((inst >> 3) & 0b11) << 7)
                        | (((inst >> 5) & 1) << 6)
                        | (((inst >> 2) & 1) << 5)
                        | (((inst >> 6) & 1) << 4);
                    let imm_sign = (((imm as i32) << 22) >> 22) as u32;
                    Some(0x13 | (2 << 7) | (2 << 15) | ((imm_sign & 0xFFF) << 20))
                } else if rd != 0 {
                    // C.LUI: lui rd, nzuimm
                    let imm = (((inst >> 12) & 1) << 5) | ((inst >> 2) & 0x1F);
                    let imm_sign = (((imm as i32) << 26) >> 26) as u32;
                    Some(0x37 | (rd << 7) | ((imm_sign & 0xFFFFF) << 12))
                } else {
                    None
                }
            }

            // C.MISC-ALU
            0b100 => {
                let rd_prime = cr(inst >> 7);
                let bit11_10 = (inst >> 10) & 0b11;
                let bit12 = (inst >> 12) & 1;

                match bit11_10 {
                    // C.SRLI
                    0b00 => {
                        let shamt = ((bit12 << 5) | ((inst >> 2) & 0x1F)) & 0x3F;
                        Some(0x13 | (rd_prime << 7) | (0b101 << 12) | (rd_prime << 15) | (shamt << 20))
                    }
                    // C.SRAI
                    0b01 => {
                        let shamt = ((bit12 << 5) | ((inst >> 2) & 0x1F)) & 0x3F;
                        Some(0x13 | (rd_prime << 7) | (0b101 << 12) | (rd_prime << 15) | (shamt << 20) | (0x20 << 25))
                    }
                    // C.ANDI
                    0b10 => {
                        let imm = (bit12 << 5) | ((inst >> 2) & 0x1F);
                        let imm_sign = (((imm as i32) << 26) >> 26) as u32;
                        Some(0x13 | (rd_prime << 7) | (0b111 << 12) | (rd_prime << 15) | ((imm_sign & 0xFFF) << 20))
                    }
                    // Register-Register
                    0b11 => {
                        let rs2_prime = cr(inst >> 2);
                        let subfunct = ((inst >> 5) & 0b11) | (bit12 << 2);
                        match subfunct {
                            0b000 => Some(0x33 | (rd_prime << 7) | (0b000 << 12) | (rd_prime << 15) | (rs2_prime << 20) | (0x20 << 25)), // SUB
                            0b001 => Some(0x33 | (rd_prime << 7) | (0b100 << 12) | (rd_prime << 15) | (rs2_prime << 20)), // XOR
                            0b010 => Some(0x33 | (rd_prime << 7) | (0b110 << 12) | (rd_prime << 15) | (rs2_prime << 20)), // OR
                            0b011 => Some(0x33 | (rd_prime << 7) | (0b111 << 12) | (rd_prime << 15) | (rs2_prime << 20)), // AND
                            0b100 => Some(0x3B | (rd_prime << 7) | (0b000 << 12) | (rd_prime << 15) | (rs2_prime << 20) | (0x20 << 25)), // SUBW
                            0b101 => Some(0x3B | (rd_prime << 7) | (0b000 << 12) | (rd_prime << 15) | (rs2_prime << 20)), // ADDW
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }

            // C.J: jal x0, offset
            0b101 => {
                let imm = (((inst >> 12) & 1) << 11)
                    | (((inst >> 8) & 1) << 10)
                    | (((inst >> 9) & 0b11) << 8)
                    | (((inst >> 6) & 1) << 7)
                    | (((inst >> 7) & 1) << 6)
                    | (((inst >> 2) & 1) << 5)
                    | (((inst >> 11) & 1) << 4)
                    | (((inst >> 3) & 0b111) << 1);
                let imm_sign = (((imm as i32) << 20) >> 20) as u32;
                let imm20 = ((imm_sign >> 20) & 1) << 31;
                let imm10_1 = ((imm_sign >> 1) & 0x3FF) << 21;
                let imm11 = ((imm_sign >> 11) & 1) << 20;
                let imm19_12 = ((imm_sign >> 12) & 0xFF) << 12;
                Some(0x6F | (0 << 7) | imm19_12 | imm11 | imm10_1 | imm20)
            }

            // C.BEQZ: beq rs1', x0, offset
            0b110 => {
                let rs1_prime = cr(inst >> 7);
                let imm = (((inst >> 12) & 1) << 8)
                    | (((inst >> 5) & 0b11) << 6)
                    | (((inst >> 2) & 1) << 5)
                    | (((inst >> 10) & 0b11) << 3)
                    | (((inst >> 3) & 0b11) << 1);
                let imm_sign = (((imm as i32) << 23) >> 23) as u32;
                let imm12 = (imm_sign >> 12) & 1;
                let imm11 = (imm_sign >> 11) & 1;
                let imm10_5 = (imm_sign >> 5) & 0x3F;
                let imm4_1 = (imm_sign >> 1) & 0xF;
                Some(0x63 | (imm11 << 7) | (imm4_1 << 8) | (0b000 << 12) | (rs1_prime << 15) | (0 << 20) | (imm10_5 << 25) | (imm12 << 31))
            }

            // C.BNEZ: bne rs1', x0, offset
            0b111 => {
                let rs1_prime = cr(inst >> 7);
                let imm = (((inst >> 12) & 1) << 8)
                    | (((inst >> 5) & 0b11) << 6)
                    | (((inst >> 2) & 1) << 5)
                    | (((inst >> 10) & 0b11) << 3)
                    | (((inst >> 3) & 0b11) << 1);
                let imm_sign = (((imm as i32) << 23) >> 23) as u32;
                let imm12 = (imm_sign >> 12) & 1;
                let imm11 = (imm_sign >> 11) & 1;
                let imm10_5 = (imm_sign >> 5) & 0x3F;
                let imm4_1 = (imm_sign >> 1) & 0xF;
                Some(0x63 | (imm11 << 7) | (imm4_1 << 8) | (0b001 << 12) | (rs1_prime << 15) | (0 << 20) | (imm10_5 << 25) | (imm12 << 31))
            }

            _ => None,
        },

        // Quadrant 10
        0b10 => match funct3 {
            // C.SLLI: slli rd, rd, shamt
            0b000 => {
                let rd = (inst >> 7) & 0x1F;
                if rd == 0 {
                    return None;
                }
                let shamt = (((inst >> 12) & 1) << 5) | ((inst >> 2) & 0x1F);
                Some(0x13 | (rd << 7) | (0b001 << 12) | (rd << 15) | (shamt << 20))
            }

            // C.FLDSP: fld rd, offset(x2)
            0b001 => {
                let rd = (inst >> 7) & 0x1F;
                let offset = (((inst >> 5) & 0b11) << 3)
                    | (((inst >> 12) & 1) << 5)
                    | (((inst >> 2) & 0b111) << 6);
                Some(0x07 | (rd << 7) | (0b011 << 12) | (2 << 15) | (offset << 20))
            }

            // C.LWSP: lw rd, offset(x2)
            0b010 => {
                let rd = (inst >> 7) & 0x1F;
                if rd == 0 {
                    return None;
                }
                let offset = (((inst >> 4) & 0b111) << 2)
                    | (((inst >> 12) & 1) << 5)
                    | (((inst >> 2) & 0b11) << 6);
                Some(0x03 | (rd << 7) | (0b010 << 12) | (2 << 15) | (offset << 20))
            }

            // C.LDSP: ld rd, offset(x2)
            0b011 => {
                let rd = (inst >> 7) & 0x1F;
                if rd == 0 {
                    return None;
                }
                let offset = (((inst >> 5) & 0b11) << 3)
                    | (((inst >> 12) & 1) << 5)
                    | (((inst >> 2) & 0b111) << 6);
                Some(0x03 | (rd << 7) | (0b011 << 12) | (2 << 15) | (offset << 20))
            }

            // C.JR / C.MV / C.JALR / C.ADD
            0b100 => {
                let bit12 = (inst >> 12) & 1;
                let rd_rs1 = (inst >> 7) & 0x1F;
                let rs2 = (inst >> 2) & 0x1F;

                if bit12 == 0 {
                    if rs2 == 0 {
                        // C.JR: jalr x0, rs1, 0
                        if rd_rs1 == 0 {
                            return None;
                        }
                        Some(0x67 | (0 << 7) | (0b000 << 12) | (rd_rs1 << 15))
                    } else {
                        // C.MV: add rd, x0, rs2
                        Some(0x33 | (rd_rs1 << 7) | (0b000 << 12) | (0 << 15) | (rs2 << 20))
                    }
                } else {
                    if rs2 == 0 {
                        // C.JALR: jalr x1, rs1, 0
                        if rd_rs1 == 0 {
                            return None; // EBREAK if rd_rs1 == 0
                        }
                        Some(0x67 | (1 << 7) | (0b000 << 12) | (rd_rs1 << 15))
                    } else {
                        // C.ADD: add rd, rd, rs2
                        Some(0x33 | (rd_rs1 << 7) | (0b000 << 12) | (rd_rs1 << 15) | (rs2 << 20))
                    }
                }
            }

            // C.FSDSP: fsd rs2, offset(x2)
            0b101 => {
                let rs2 = (inst >> 2) & 0x1F;
                let offset = (((inst >> 10) & 0b111) << 3) | (((inst >> 7) & 0b111) << 6);
                let imm4_0 = offset & 0x1F;
                let imm11_5 = (offset >> 5) & 0x7F;
                Some(0x27 | (imm4_0 << 7) | (0b011 << 12) | (2 << 15) | (rs2 << 20) | (imm11_5 << 25))
            }

            // C.SWSP: sw rs2, offset(x2)
            0b110 => {
                let rs2 = (inst >> 2) & 0x1F;
                let offset = (((inst >> 9) & 0b1111) << 2) | (((inst >> 7) & 0b11) << 6);
                let imm4_0 = offset & 0x1F;
                let imm11_5 = (offset >> 5) & 0x7F;
                Some(0x23 | (imm4_0 << 7) | (0b010 << 12) | (2 << 15) | (rs2 << 20) | (imm11_5 << 25))
            }

            // C.SDSP: sd rs2, offset(x2)
            0b111 => {
                let rs2 = (inst >> 2) & 0x1F;
                let offset = (((inst >> 10) & 0b111) << 3) | (((inst >> 7) & 0b111) << 6);
                let imm4_0 = offset & 0x1F;
                let imm11_5 = (offset >> 5) & 0x7F;
                Some(0x23 | (imm4_0 << 7) | (0b011 << 12) | (2 << 15) | (rs2 << 20) | (imm11_5 << 25))
            }

            _ => None,
        },

        _ => None,
    }
}
