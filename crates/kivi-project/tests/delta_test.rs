use kivi_core::memory_map::MemoryMap;
use kivi_project::delta::{
    commit_changes, compute_diff, rollback, CellDelta, Commit, ProjectError, ProjectSession,
};

#[test]
fn test_commit_changes() {
    let mut session = ProjectSession::new(vec![0x00; 100]);
    session.current_rom[10] = 0x55;
    let commit_id = session.commit("Bumped fuel table byte", 1700000000);
    assert_eq!(commit_id, 1);
    assert_eq!(session.history.len(), 1);
    assert_eq!(session.history[0].id, 1);
    assert_eq!(session.history[0].message, "Bumped fuel table byte");
    assert_eq!(session.history[0].timestamp, 1700000000);
    assert_eq!(session.history[0].deltas.len(), 1);
    assert_eq!(session.history[0].deltas[0].address, 10);
    assert_eq!(session.history[0].deltas[0].old_raw, 0x00);
    assert_eq!(session.history[0].deltas[0].new_raw, 0x55);
}

#[test]
fn test_commit_multiple_incremental() {
    let mut session = ProjectSession::new(vec![0x00; 100]);

    // Commit 1: byte 10 -> 0x55
    session.current_rom[10] = 0x55;
    let c1 = session.commit("First commit", 100);
    assert_eq!(c1, 1);
    assert_eq!(session.history[0].deltas.len(), 1);
    assert_eq!(session.history[0].deltas[0].address, 10);
    assert_eq!(session.history[0].deltas[0].old_raw, 0x00);
    assert_eq!(session.history[0].deltas[0].new_raw, 0x55);

    // Commit 2: byte 20 -> 0xAA
    session.current_rom[20] = 0xAA;
    let c2 = session.commit("Second commit", 200);
    assert_eq!(c2, 2);
    // Delta should only be for byte 20, not byte 10
    assert_eq!(session.history[1].deltas.len(), 1);
    assert_eq!(session.history[1].deltas[0].address, 20);
    assert_eq!(session.history[1].deltas[0].old_raw, 0x00);
    assert_eq!(session.history[1].deltas[0].new_raw, 0xAA);

    // Commit 3: update byte 10 to 0x60
    session.current_rom[10] = 0x60;
    let c3 = session.commit("Third commit", 300);
    assert_eq!(c3, 3);
    assert_eq!(session.history[2].deltas.len(), 1);
    assert_eq!(session.history[2].deltas[0].address, 10);
    assert_eq!(session.history[2].deltas[0].old_raw, 0x55);
    assert_eq!(session.history[2].deltas[0].new_raw, 0x60);
}

#[test]
fn test_rollback_to_commit_and_original() {
    let mut session = ProjectSession::new(vec![0x00; 100]);

    // Commit 1: byte 5 = 0x11
    session.current_rom[5] = 0x11;
    session.commit("C1", 1);

    // Commit 2: byte 10 = 0x22
    session.current_rom[10] = 0x22;
    session.commit("C2", 2);

    // Commit 3: byte 15 = 0x33
    session.current_rom[15] = 0x33;
    session.commit("C3", 3);

    // Uncommitted change: byte 20 = 0x44
    session.current_rom[20] = 0x44;

    // Rollback to Commit 2: byte 5 should be 0x11, byte 10 should be 0x22,
    // byte 15 and 20 should be 0x00.
    session.rollback(2).expect("Rollback to C2 should succeed");
    assert_eq!(session.current_rom[5], 0x11);
    assert_eq!(session.current_rom[10], 0x22);
    assert_eq!(session.current_rom[15], 0x00);
    assert_eq!(session.current_rom[20], 0x00);

    // Rollback to Commit 0 (Original ROM)
    session.rollback(0).expect("Rollback to C0 should succeed");
    assert_eq!(session.current_rom, session.original_rom);

    // Rollback to Commit 3 using standalone helper
    rollback(&mut session, 3).expect("Standalone rollback to C3 should succeed");
    assert_eq!(session.current_rom[5], 0x11);
    assert_eq!(session.current_rom[10], 0x22);
    assert_eq!(session.current_rom[15], 0x33);
    assert_eq!(session.current_rom[20], 0x00);

    // Rollback to original helper
    session.rollback_to_original();
    assert_eq!(session.current_rom, session.original_rom);
}

#[test]
fn test_rollback_invalid_id() {
    let mut session = ProjectSession::new(vec![0x00; 10]);
    let err = session.rollback(99).unwrap_err();
    assert_eq!(err, ProjectError::CommitNotFound(99));
}

#[test]
fn test_diffing() {
    let a = vec![0x01, 0x02, 0x03, 0x04];
    let b = vec![0x01, 0xFF, 0x03, 0xAA];
    let deltas = compute_diff(&a, &b);
    assert_eq!(deltas.len(), 2);
    assert_eq!(
        deltas[0],
        CellDelta {
            address: 1,
            old_raw: 0x02,
            new_raw: 0xFF,
        }
    );
    assert_eq!(
        deltas[1],
        CellDelta {
            address: 3,
            old_raw: 0x04,
            new_raw: 0xAA,
        }
    );

    let mut session = ProjectSession::new(a.clone());
    session.current_rom = b.clone();
    assert_eq!(session.diff_original(), deltas);
    assert_eq!(session.diff_uncommitted(), deltas);
    assert_eq!(session.diff(), deltas);

    session.commit("Committed changes", 100);
    assert_eq!(session.diff_uncommitted().len(), 0);
    assert_eq!(session.diff_original().len(), 2);

    session.current_rom[0] = 0x99;
    session.commit("Change byte 0", 200);

    // Diff between commit 1 and commit 2
    let diff_1_2 = session.diff_commits(1, 2).expect("diff commits 1 and 2");
    assert_eq!(diff_1_2.len(), 1);
    assert_eq!(
        diff_1_2[0],
        CellDelta {
            address: 0,
            old_raw: 0x01,
            new_raw: 0x99,
        }
    );
}

#[test]
fn test_memory_map_interoperability() {
    let bytes = vec![0xAA, 0xBB, 0xCC, 0xDD];
    let mem = MemoryMap::new(bytes.clone());

    let mut session = ProjectSession::from_memory_map(&mem);
    assert_eq!(session.original_rom, bytes);
    assert_eq!(session.current_rom, bytes);

    let from_trait: ProjectSession = (&mem).into();
    assert_eq!(from_trait.original_rom, bytes);

    session.current_rom[0] = 0x11;
    let commit_id = commit_changes(&mut session, "Modified byte 0", 12345);
    assert_eq!(commit_id, 1);

    let mem_out = session.to_memory_map();
    assert_eq!(mem_out.as_slice(), &[0x11, 0xBB, 0xCC, 0xDD]);

    let orig_mem = session.original_memory_map();
    assert_eq!(orig_mem.as_slice(), &bytes);
}

#[test]
fn test_serde_roundtrip() {
    let mut session = ProjectSession::new(vec![1, 2, 3, 4]);
    session.current_rom[2] = 99;
    session.commit("Patch byte 2", 500);

    let json = serde_json::to_string(&session).expect("serialize should succeed");
    let recovered: ProjectSession =
        serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(recovered.original_rom, session.original_rom);
    assert_eq!(recovered.current_rom, session.current_rom);
    assert_eq!(recovered.history, session.history);

    let expected_commit = Commit {
        id: 1,
        message: "Patch byte 2".to_string(),
        timestamp: 500,
        deltas: vec![CellDelta {
            address: 2,
            old_raw: 3,
            new_raw: 99,
        }],
    };
    assert_eq!(session.history[0], expected_commit);
}
