//! B4a: user-supplied base address for the loaded workspace ROM.
//!
//! A raw `.bin` has no inherent address, so the workspace map starts with
//! `None`. These tests cover the command that lets a user *state* the address
//! explicitly — and prove that stating nothing keeps it unknown.

use kivituner::commands::{
    get_rom_base_address_inner, load_bin_data_inner, set_rom_base_address_inner,
};
use kivituner::AppState;

fn loaded_state() -> AppState {
    let state = AppState::default();
    load_bin_data_inner(&[1, 2, 3, 4, 5, 6, 7, 8], &state).expect("load");
    state
}

#[test]
fn freshly_loaded_bin_has_unknown_base() {
    let state = loaded_state();
    assert_eq!(
        get_rom_base_address_inner(&state).expect("get"),
        None,
        "a raw BIN must not be assumed to start at zero"
    );
}

#[test]
fn setting_a_nonzero_base_is_readable_back() {
    let state = loaded_state();
    set_rom_base_address_inner(Some(0x8000), &state).expect("set");
    assert_eq!(get_rom_base_address_inner(&state).expect("get"), Some(0x8000));
}

#[test]
fn setting_known_zero_is_distinct_from_unknown() {
    let state = loaded_state();
    set_rom_base_address_inner(Some(0), &state).expect("set");
    assert_eq!(
        get_rom_base_address_inner(&state).expect("get"),
        Some(0),
        "known zero must survive as Some(0)"
    );
}

#[test]
fn clearing_the_base_returns_it_to_unknown() {
    let state = loaded_state();
    set_rom_base_address_inner(Some(0x4000), &state).expect("set");
    set_rom_base_address_inner(None, &state).expect("clear");
    assert_eq!(get_rom_base_address_inner(&state).expect("get"), None);
}

#[test]
fn setting_a_base_does_not_alter_rom_bytes_or_hashes() {
    let state = loaded_state();
    let before = {
        let g = state.memory.lock().unwrap();
        let m = g.as_ref().unwrap();
        (m.as_slice().to_vec(), m.hashes().clone())
    };

    set_rom_base_address_inner(Some(0x1234), &state).expect("set");

    let after = {
        let g = state.memory.lock().unwrap();
        let m = g.as_ref().unwrap();
        (m.as_slice().to_vec(), m.hashes().clone())
    };

    assert_eq!(before.0, after.0, "ROM bytes must not change");
    assert_eq!(before.1, after.1, "hashes must not change");
}

#[test]
fn setting_a_base_without_a_loaded_rom_is_an_error() {
    let state = AppState::default();
    let err = set_rom_base_address_inner(Some(0x1000), &state).expect_err("must fail");
    assert_eq!(err, "No binary loaded");
}

#[test]
fn reading_a_base_without_a_loaded_rom_is_an_error() {
    let state = AppState::default();
    let err = get_rom_base_address_inner(&state).expect_err("must fail");
    assert_eq!(err, "No binary loaded");
}

#[test]
fn reloading_a_rom_clears_a_previously_set_base() {
    // Guards against stale metadata surviving a file change.
    let state = loaded_state();
    set_rom_base_address_inner(Some(0x9000), &state).expect("set");

    load_bin_data_inner(&[9, 9, 9, 9], &state).expect("reload");

    assert_eq!(
        get_rom_base_address_inner(&state).expect("get"),
        None,
        "a newly loaded ROM must not inherit the previous address"
    );
}
