//! Ported from `ecu_engine_rust::flashing::bytecode_security_vm`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

use std::collections::VecDeque;

/// VAG SA2 Bytecode Virtual Machine State & Interpreter.
#[derive(Debug, Clone)]
pub struct VagSa2BytecodeVm {
    pub script: Vec<u8>,
    pub max_steps: usize,
}

impl VagSa2BytecodeVm {
    pub fn new(script: Vec<u8>) -> Self {
        Self {
            script,
            max_steps: 4096,
        }
    }

    /// Execute the VAG SA2 bytecode program on a 32-bit Seed to compute the 32-bit Security Key.
    /// Canonical instruction set matching `crate::sa2_seed_key::Sa2SeedKey`:
    /// - `0x81`: RSL (Rotate Shift Left 1 bit, MSB -> carry)
    /// - `0x82`: RSR (Rotate Shift Right 1 bit, LSB -> carry)
    /// - `0x93 <b0..b3>`: ADD 32-bit big-endian immediate (sets carry on overflow)
    /// - `0x84 <b0..b3>`: SUB 32-bit big-endian immediate (sets carry on borrow)
    /// - `0x87 <b0..b3>`: EOR 32-bit big-endian immediate
    /// - `0x68 <count>`: FOR loop start (`count` iterations)
    /// - `0x49`: NEXT loop end
    /// - `0x4A <off>`: BCC skip `off + 2` bytes when carry is clear
    /// - `0x6B <off>`: BRA skip `off + 2` bytes unconditionally
    /// - `0x4C`: FINISH / RET
    pub fn execute(&self, seed: u32) -> Result<u32, &'static str> {
        let code = &self.script;
        if code.is_empty() {
            return Ok(seed);
        }

        let mut reg: u32 = seed;
        let mut carry = false;
        let mut ip: usize = 0;
        let mut loop_counters: VecDeque<u8> = VecDeque::with_capacity(8);
        let mut loop_starts: VecDeque<usize> = VecDeque::with_capacity(8);
        let mut steps: usize = 0;

        while ip < code.len() {
            steps += 1;
            if steps > self.max_steps {
                return Err("SA2 VM execution exceeded maximum step limit (infinite loop guard)");
            }

            let op = code[ip];
            match op {
                0x81 => {
                    carry = (reg & 0x8000_0000) != 0;
                    reg = reg.rotate_left(1);
                    ip += 1;
                }
                0x82 => {
                    carry = (reg & 1) != 0;
                    reg = reg.rotate_right(1);
                    ip += 1;
                }
                0x93 => {
                    if ip + 4 >= code.len() {
                        return Err("SA2 VM truncated 0x93 ADD instruction");
                    }
                    let imm = u32::from_be_bytes([code[ip + 1], code[ip + 2], code[ip + 3], code[ip + 4]]);
                    let (value, overflow) = reg.overflowing_add(imm);
                    carry = overflow;
                    reg = value;
                    ip += 5;
                }
                0x84 => {
                    if ip + 4 >= code.len() {
                        return Err("SA2 VM truncated 0x84 SUB instruction");
                    }
                    let imm = u32::from_be_bytes([code[ip + 1], code[ip + 2], code[ip + 3], code[ip + 4]]);
                    let (value, borrow) = reg.overflowing_sub(imm);
                    carry = borrow;
                    reg = value;
                    ip += 5;
                }
                0x87 => {
                    if ip + 4 >= code.len() {
                        return Err("SA2 VM truncated 0x87 EOR instruction");
                    }
                    let imm = u32::from_be_bytes([code[ip + 1], code[ip + 2], code[ip + 3], code[ip + 4]]);
                    reg ^= imm;
                    ip += 5;
                }
                0x68 => {
                    if ip + 1 >= code.len() {
                        return Err("SA2 VM truncated 0x68 FOR instruction");
                    }
                    let iterations = code[ip + 1];
                    loop_counters.push_front(iterations.saturating_sub(1));
                    ip += 2;
                    loop_starts.push_front(ip);
                }
                0x49 => {
                    let Some(counter) = loop_counters.front_mut() else {
                        return Err("SA2 VM 0x49 NEXT without matching 0x68 FOR");
                    };
                    if *counter > 0 {
                        *counter -= 1;
                        ip = *loop_starts
                            .front()
                            .ok_or("SA2 VM 0x49 NEXT without matching 0x68 FOR")?;
                    } else {
                        loop_counters.pop_front();
                        loop_starts.pop_front();
                        ip += 1;
                    }
                }
                0x4A => {
                    if ip + 1 >= code.len() {
                        return Err("SA2 VM truncated 0x4A BCC instruction");
                    }
                    let skip = code[ip + 1] as usize + 2;
                    if carry {
                        ip += 2;
                    } else {
                        ip += skip;
                    }
                }
                0x6B => {
                    if ip + 1 >= code.len() {
                        return Err("SA2 VM truncated 0x6B BRA instruction");
                    }
                    let skip = code[ip + 1] as usize + 2;
                    ip += skip;
                }
                0x4C => {
                    break;
                }
                _ => {
                    return Err("Unknown SA2 bytecode instruction encountered");
                }
            }
        }

        Ok(reg)
    }

    /// Scan a CBOOT or Flash binary for an embedded VAG SA2 bytecode script.
    /// Looks for canonical SA2 header `0x68` (FOR loop) followed by shift/arithmetic ops and terminated by `0x49 0x4C`.
    pub fn extract_sa2_bytecode_from_cboot(binary: &[u8]) -> Option<(usize, Vec<u8>)> {
        if binary.len() < 8 {
            return None;
        }
        for i in 0..(binary.len() - 8) {
            if binary[i] == 0x68 && (1..=32).contains(&binary[i + 1]) {
                let next_op = binary[i + 2];
                if matches!(next_op, 0x81 | 0x82 | 0x93 | 0x84 | 0x87 | 0x4A | 0x6B) {
                    let max_end = (i + 96).min(binary.len());
                    for end in (i + 4)..max_end {
                        if binary[end - 1] == 0x49 && binary[end] == 0x4C {
                            return Some((i, binary[i..=end].to_vec()));
                        }
                    }
                }
            }
        }
        None
    }
}

/// GM 256-Algorithm Seed-Key Bytecode Virtual Machine (Multi-Algorithm Seed-Key Engine).
/// Executes a 3-byte instruction tuple `[opcode, ll, hh]` aligned with `crate::gm_seed_key_tables`.
pub struct GmSeedKeyBytecodeVm;

impl GmSeedKeyBytecodeVm {
    pub const OP_SWAP_BYTES: u8 = 0x05;
    pub const OP_ADD_WORD: u8 = 0x14;
    pub const OP_COMPLEMENT: u8 = 0x2A;
    pub const OP_AND_WORD: u8 = 0x37;
    pub const OP_ROL_BITS: u8 = 0x4C;
    pub const OP_OR_WORD: u8 = 0x52;
    pub const OP_ROR_BITS: u8 = 0x6B;
    pub const OP_ADD_WORD_LE: u8 = 0x75;
    pub const OP_SWAP_ADD: u8 = 0x7E;
    pub const OP_SUB_WORD: u8 = 0x98;
    pub const OP_SUB_WORD_LE: u8 = 0xF8;
    pub const OP_TERM: u8 = 0xFF;

    /// Execute GM CAN Seed-Key bytecode array on a 16-bit seed.
    /// Each instruction is 3 bytes `[opcode, ll, hh]`. Execution stops at end of slice or `0xFF`.
    pub fn compute_key_u16(seed: u16, bytecode: &[u8]) -> Result<u16, &'static str> {
        let mut acc = seed;
        let mut idx = 0;

        while idx < bytecode.len() {
            let op = bytecode[idx];
            if op == Self::OP_TERM {
                break;
            }
            if idx + 2 >= bytecode.len() {
                return Err("Truncated GM Seed-Key 3-byte instruction tuple");
            }
            let ll = bytecode[idx + 1];
            let hh = bytecode[idx + 2];
            let hhll = ((hh as u16) << 8) | (ll as u16);
            let llhh = ((ll as u16) << 8) | (hh as u16);

            match op {
                Self::OP_SWAP_BYTES => {
                    acc = acc.swap_bytes();
                }
                Self::OP_ADD_WORD => {
                    acc = acc.wrapping_add(llhh);
                }
                Self::OP_COMPLEMENT => {
                    acc = if ll < hh {
                        (!acc).wrapping_add(1)
                    } else {
                        !acc
                    };
                }
                Self::OP_AND_WORD => {
                    acc &= hhll;
                }
                Self::OP_ROL_BITS => {
                    acc = acc.rotate_left((ll & 15) as u32);
                }
                Self::OP_OR_WORD => {
                    acc |= hhll;
                }
                Self::OP_ROR_BITS => {
                    acc = acc.rotate_right((hh & 15) as u32);
                }
                Self::OP_ADD_WORD_LE => {
                    acc = acc.wrapping_add(hhll);
                }
                Self::OP_SWAP_ADD => {
                    let swapped = acc.swap_bytes();
                    let operand = if ll >= hh { llhh } else { hhll };
                    acc = swapped.wrapping_add(operand);
                }
                Self::OP_SUB_WORD => {
                    acc = acc.wrapping_sub(llhh);
                }
                Self::OP_SUB_WORD_LE => {
                    acc = acc.wrapping_sub(hhll);
                }
                _ => return Err("Unsupported GM Seed-Key opcode"),
            }

            idx += 3;
        }

        Ok(acc)
    }
}
