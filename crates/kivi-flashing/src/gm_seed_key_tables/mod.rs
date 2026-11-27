//! GM Delco VPW, GMLAN, and GM-Other 16-bit seed/key bytecode engines.
//!
//! Executes compact 13-byte seed/key stack-bytecode programs indexed by algorithm ID.

pub mod gm_other_table;
pub mod gmlan_table;
pub mod vpw_table;

pub const GM_OTHER_ALGO_BASE: u16 = 0x100;
pub const E92_LEGACY_CAN_ALGO: u16 = GM_OTHER_ALGO_BASE + 1;

/// Computes the 16-bit unlock key for GM Class-2 VPW PCMs (`algo` in `0..=255`).
pub fn gm_vpw_key(algo: u8, seed: u16) -> u16 {
    if seed == 0xFFFF {
        return 0xFFFF;
    }
    let prog = &vpw_table::GM_VPW_KEY_TABLE[algo as usize];
    let mut key = seed;
    let mut idx = 1usize;
    for _ in 0..4 {
        let op = prog[idx];
        let hi = prog[idx + 1] as u16;
        let lo = prog[idx + 2] as u16;
        match op {
            20 => {
                // Op_code_add
                key = key.wrapping_add((hi << 8) | lo);
            }
            42 => {
                // Op_code_comp
                key = if hi > lo {
                    !key
                } else {
                    (!key).wrapping_add(1)
                };
            }
            76 => {
                // Op_code_rot_lt
                let shift = (hi as u32) & 31;
                key = ((key as u32).wrapping_shl(shift) | (key as u32).wrapping_shr((16 - hi as u32) & 31)) as u16;
            }
            107 => {
                // Op_code_rot_rt
                let shift = (lo as u32) & 31;
                key = ((key as u32).wrapping_shr(shift) | (key as u32).wrapping_shl((16 - lo as u32) & 31)) as u16;
            }
            126 => {
                // Op_code_swap_add
                let swapped = key.swap_bytes();
                let imm = if hi >= lo { (hi << 8) | lo } else { (lo << 8) | hi };
                key = swapped.wrapping_add(imm);
            }
            152 => {
                // Op_code_sub
                key = key.wrapping_sub((hi << 8) | lo);
            }
            _ => {}
        }
        idx += 3;
    }
    key
}

fn run_can_key_program(prog: &[u8; 13], seed: u16) -> u16 {
    let mut key = seed;
    let mut base = 0usize;
    for _ in 0..4 {
        let op = prog[base];
        let b0 = prog[base + 1] as u16;
        let b1 = prog[base + 2] as u16;
        if op > 0x52 {
            match op {
                0x6B => {
                    let shift = (b1 as u32) & 31;
                    key = ((key as u32).wrapping_shr(shift) | (key as u32).wrapping_shl((16 - b1 as u32) & 31)) as u16;
                }
                0x75 => key = key.wrapping_add(b0 | (b1 << 8)),
                0x7E => {
                    let swapped = key.swap_bytes();
                    let operand = if b0 >= b1 { (b0 << 8) | b1 } else { (b1 << 8) | b0 };
                    key = swapped.wrapping_add(operand);
                }
                0x98 => key = key.wrapping_sub((b0 << 8) | b1),
                0xF8 => key = key.wrapping_sub(b0 | (b1 << 8)),
                _ => {}
            }
        } else {
            match op {
                0x05 => key = key.swap_bytes(),
                0x14 => key = key.wrapping_add((b0 << 8) | b1),
                0x2A => {
                    key = if b0 < b1 {
                        (!key).wrapping_add(1)
                    } else {
                        !key
                    };
                }
                0x37 => key &= b0 | (b1 << 8),
                0x4C => {
                    let shift = (b0 as u32) & 31;
                    key = ((key as u32).wrapping_shl(shift) | (key as u32).wrapping_shr((16 - b0 as u32) & 31)) as u16;
                }
                0x52 => key |= b0 | (b1 << 8),
                _ => {}
            }
        }
        base += 3;
    }
    key
}

/// Computes the 16-bit unlock key for GM GMLAN (`algo < 0x100`) or GM-Other (`algo >= 0x100`).
pub fn gm_can_key(algo: u16, seed: u16) -> u16 {
    if seed == 0xFFFF {
        return 0xFFFF;
    }
    if algo >= GM_OTHER_ALGO_BASE {
        let idx = (algo - GM_OTHER_ALGO_BASE) as usize;
        if idx >= gm_other_table::GM_OTHER_KEY_TABLE.len() {
            return 0;
        }
        return run_can_key_program(&gm_other_table::GM_OTHER_KEY_TABLE[idx], seed);
    }
    let idx = algo as usize;
    if idx >= gmlan_table::GM_GMLAN_KEY_TABLE.len() {
        return 0;
    }
    run_can_key_program(&gmlan_table::GM_GMLAN_KEY_TABLE[idx], seed)
}
