//! VAG SA2 seed/key bytecode machine.
//!
//! ## Why a bytecode machine and not a formula
//!
//! VAG control units using SA2 ship their SecurityAccess algorithm as a small
//! **program** in the flash container (FRF/ODX). The ECU sends a seed, the tool
//! runs the program, and the result is the key. The algorithm therefore differs
//! per ECU, which is why a single fixed formula in [`crate::uds`] cannot unlock
//! these units.
//!
//! ## Instruction set
//!
//! | Opcode | Name | Effect |
//! |---|---|---|
//! | `0x81` | RSL | rotate register left through carry |
//! | `0x82` | RSR | rotate register right through carry |
//! | `0x93` | ADD | register += u32 operand, set carry on overflow |
//! | `0x84` | SUB | register -= u32 operand, set carry on borrow |
//! | `0x87` | EOR | register ^= u32 operand |
//! | `0x68` | FOR | push a loop counter (operand − 1) and the loop start |
//! | `0x49` | NEXT | decrement; jump back to the loop start, else pop |
//! | `0x4A` | BCC | skip `operand + 2` bytes when carry is clear |
//! | `0x6B` | BRA | skip `operand + 2` bytes unconditionally |
//! | `0x4C` | FINISH | stop |
//!
//! Ported from the `sa2_seed_key` reference implementation. Every divergence is
//! a defect: a wrong key means the ECU refuses the session.
//!
//! ## Safety
//!
//! The reference implementation loops until the instruction pointer runs past
//! the tape, so a malformed program can hang. This port enforces a step limit
//! and returns [`Sa2Error::StepLimitExceeded`] instead.

use std::collections::VecDeque;

/// Why an SA2 program could not be executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sa2Error {
    /// The tape has no instructions.
    EmptyTape,
    /// An opcode outside the documented set.
    UnknownOpcode { opcode: u8 },
    /// An instruction needed more operand bytes than remain.
    TruncatedOperand { opcode: u8, needed: usize, remaining: usize },
    /// `NEXT` executed without a matching `FOR`.
    UnmatchedNext,
    /// Execution exceeded [`STEP_LIMIT`], which only a malformed program does.
    StepLimitExceeded,
}

impl std::fmt::Display for Sa2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sa2Error::EmptyTape => f.write_str("the SA2 instruction tape is empty"),
            Sa2Error::UnknownOpcode { opcode } => {
                write!(f, "unknown SA2 opcode {opcode:#04X}")
            }
            Sa2Error::TruncatedOperand {
                opcode,
                needed,
                remaining,
            } => write!(
                f,
                "SA2 opcode {opcode:#04X} needs {needed} operand bytes but only {remaining} remain"
            ),
            Sa2Error::UnmatchedNext => f.write_str("SA2 NEXT without a matching FOR"),
            Sa2Error::StepLimitExceeded => {
                f.write_str("SA2 program exceeded its step limit (malformed tape)")
            }
        }
    }
}

impl std::error::Error for Sa2Error {}

/// Maximum instructions executed before a program is declared malformed.
///
/// Real SA2 tapes are a few dozen instructions; this bound is far above any
/// legitimate program and exists only to stop a runaway branch.
pub const STEP_LIMIT: usize = 100_000;

// Opcodes.
const OP_RSL: u8 = 0x81;
const OP_RSR: u8 = 0x82;
const OP_ADD: u8 = 0x93;
const OP_SUB: u8 = 0x84;
const OP_EOR: u8 = 0x87;
const OP_FOR: u8 = 0x68;
const OP_NEXT: u8 = 0x49;
const OP_BCC: u8 = 0x4A;
const OP_BRA: u8 = 0x6B;
const OP_FINISH: u8 = 0x4C;

/// The SA2 machine: a register, a carry flag, and a program counter.
#[derive(Debug, Clone)]
pub struct Sa2SeedKey<'a> {
    tape: &'a [u8],
    register: u32,
    carry: bool,
    pc: usize,
    loop_counters: VecDeque<u8>,
    loop_starts: VecDeque<usize>,
}

impl<'a> Sa2SeedKey<'a> {
    /// Creates a machine that will run `tape` against `seed`.
    pub fn new(tape: &'a [u8], seed: u32) -> Self {
        Self {
            tape,
            register: seed,
            carry: false,
            pc: 0,
            loop_counters: VecDeque::new(),
            loop_starts: VecDeque::new(),
        }
    }

    /// Reads a 4-byte big-endian operand at `pc + 1`.
    fn operand_u32(&self) -> Result<u32, Sa2Error> {
        let opcode = self.tape[self.pc];
        let remaining = self.tape.len().saturating_sub(self.pc + 1);
        if remaining < 4 {
            return Err(Sa2Error::TruncatedOperand {
                opcode,
                needed: 4,
                remaining,
            });
        }
        let o = self.pc + 1;
        Ok(u32::from_be_bytes([
            self.tape[o],
            self.tape[o + 1],
            self.tape[o + 2],
            self.tape[o + 3],
        ]))
    }

    /// Reads a 1-byte operand at `pc + 1`.
    fn operand_u8(&self) -> Result<u8, Sa2Error> {
        let opcode = self.tape[self.pc];
        let remaining = self.tape.len().saturating_sub(self.pc + 1);
        if remaining < 1 {
            return Err(Sa2Error::TruncatedOperand {
                opcode,
                needed: 1,
                remaining,
            });
        }
        Ok(self.tape[self.pc + 1])
    }

    /// Runs the program and returns the resulting key.
    pub fn execute(mut self) -> Result<u32, Sa2Error> {
        if self.tape.is_empty() {
            return Err(Sa2Error::EmptyTape);
        }
        let mut steps = 0usize;
        while self.pc < self.tape.len() {
            steps += 1;
            if steps > STEP_LIMIT {
                return Err(Sa2Error::StepLimitExceeded);
            }
            match self.tape[self.pc] {
                OP_RSL => {
                    self.carry = self.register & 0x8000_0000 != 0;
                    self.register = self.register.rotate_left(1);
                    self.pc += 1;
                }
                OP_RSR => {
                    self.carry = self.register & 1 != 0;
                    self.register = self.register.rotate_right(1);
                    self.pc += 1;
                }
                OP_ADD => {
                    let operand = self.operand_u32()?;
                    let (value, overflow) = self.register.overflowing_add(operand);
                    self.carry = overflow;
                    self.register = value;
                    self.pc += 5;
                }
                OP_SUB => {
                    let operand = self.operand_u32()?;
                    let (value, borrow) = self.register.overflowing_sub(operand);
                    self.carry = borrow;
                    self.register = value;
                    self.pc += 5;
                }
                OP_EOR => {
                    let operand = self.operand_u32()?;
                    self.register ^= operand;
                    self.pc += 5;
                }
                OP_FOR => {
                    let iterations = self.operand_u8()?;
                    // The reference pushes `iterations - 1` and jumps into the
                    // body; NEXT then runs it `iterations` times in total.
                    self.loop_counters.push_front(iterations.saturating_sub(1));
                    self.pc += 2;
                    self.loop_starts.push_front(self.pc);
                }
                OP_NEXT => {
                    let Some(counter) = self.loop_counters.front_mut() else {
                        return Err(Sa2Error::UnmatchedNext);
                    };
                    if *counter > 0 {
                        *counter -= 1;
                        self.pc = *self.loop_starts.front().ok_or(Sa2Error::UnmatchedNext)?;
                    } else {
                        self.loop_counters.pop_front();
                        self.loop_starts.pop_front();
                        self.pc += 1;
                    }
                }
                OP_BCC => {
                    let skip = self.operand_u8()? as usize + 2;
                    if self.carry {
                        self.pc += 2;
                    } else {
                        self.pc += skip;
                    }
                }
                OP_BRA => {
                    let skip = self.operand_u8()? as usize + 2;
                    self.pc += skip;
                }
                OP_FINISH => {
                    self.pc += 1;
                    break;
                }
                other => return Err(Sa2Error::UnknownOpcode { opcode: other }),
            }
        }
        Ok(self.register)
    }
}

/// Convenience: run `tape` against `seed`.
pub fn compute_sa2_key(tape: &[u8], seed: u32) -> Result<u32, Sa2Error> {
    Sa2SeedKey::new(tape, seed).execute()
}
