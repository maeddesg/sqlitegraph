//! Header Region Lockdown Tests - Phase 43
//!
//! These tests ensure the header region is protected from corruption and magic bytes remain stable.

use sqlitegraph::backend::native::{GraphFile, NativeBackendError};
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_write_bytes_rejects_header_region() {
    // Test that any write to header region is rejected with clear error
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    // Create graph file with proper header
    let mut graph_file = GraphFile::create(path).unwrap();

    // Attempt to write into header region (offset < HEADER_SIZE = 88)
    let result = graph_file.write_bytes(50, b"should_fail");

    // Must fail with specific error message
    assert!(
        result.is_err(),
        "write_bytes should reject header region writes"
    );
    match result.unwrap_err() {
        NativeBackendError::CorruptNodeRecord { node_id, reason } => {
            assert_eq!(node_id, -1, "Should use system error node_id");
            assert!(
                reason.contains("attempted write into header region"),
                "Error message should mention header region: {}",
                reason
            );
            assert!(reason.contains("offset=50"), "Should show offending offset");
            assert!(reason.contains("HEADER_SIZE=88"), "Should show HEADER_SIZE");
        }
        other => panic!(
            "Expected CorruptNodeRecord with header region message, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_write_bytes_direct_rejects_header_region() {
    // Test that write_bytes_direct also rejects header region writes
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    let mut graph_file = GraphFile::create(path).unwrap();

    // Attempt to write into header region
    let result = graph_file.write_bytes_direct(0, b"direct_header_write");

    // Must fail with specific error message
    assert!(
        result.is_err(),
        "write_bytes_direct should reject header region writes"
    );
    let error_msg = format!("{}", result.unwrap_err());
    assert!(error_msg.contains("attempted write into header region"));
    assert!(error_msg.contains("offset=0"));
}

#[test]
fn test_magic_stable_after_reopen() {
    // Test that magic bytes remain stable across close/open cycle
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    // Create and write initial header
    {
        println!("DEBUG: About to create GraphFile");
        let mut graph_file = GraphFile::create(&path).unwrap();
        println!("DEBUG: GraphFile created");
        let magic_before = graph_file.header().magic;
        println!("DEBUG: Initial magic = {:02X?}", magic_before);
        assert_eq!(magic_before, [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0]);

        // Force header write and sync
        println!("DEBUG: Before write_header: magic = {:02X?}", magic_before);
        graph_file.write_header().unwrap();
        println!(
            "DEBUG: After write_header: magic = {:02X?}",
            graph_file.header().magic
        );

        // Verify immediately after write
        let magic_after = graph_file.header().magic;
        assert_eq!(
            magic_before, magic_after,
            "Magic should be identical immediately after write"
        );
        println!("DEBUG: Header write verification: PASSED");
    }

    // Close and reopen
    let mut graph_file = GraphFile::open(&path).unwrap();
    let magic_reopened = graph_file.header().magic;

    // Magic must be unchanged after reopen
    assert_eq!(
        magic_reopened,
        [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0],
        "Magic bytes corrupted after reopen: {:02X?}",
        magic_reopened
    );
}

#[test]
fn test_magic_stable_after_cluster_writes_and_reopen() {
    // Test that magic remains stable after data writes and reopen
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    // Create graph file and add data
    {
        let mut graph_file = GraphFile::create(&path).unwrap();
        let initial_magic = graph_file.header().magic;

        // Write some data in safe region (well beyond header)
        let data_offset = 2000; // Safe from header region
        let data = b"test_cluster_data_that_should_not_corrupt_header";
        graph_file.write_bytes(data_offset, data).unwrap();

        // Verify magic unchanged after data write
        let magic_after_data = graph_file.header().magic;
        assert_eq!(
            initial_magic, magic_after_data,
            "Magic corrupted by data write"
        );

        // Write more data
        graph_file
            .write_bytes(data_offset + 100, b"more_data")
            .unwrap();

        // Final check before close
        let magic_before_close = graph_file.header().magic;
        assert_eq!(
            initial_magic, magic_before_close,
            "Magic corrupted before close"
        );
    }

    // Reopen and verify magic stability
    let mut graph_file = GraphFile::open(&path).unwrap();
    let magic_reopened = graph_file.header().magic;

    assert_eq!(
        magic_reopened,
        [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0],
        "Magic bytes corrupted after data writes and reopen: {:02X?}",
        magic_reopened
    );
}

#[test]
fn test_header_boundary_write_protection() {
    // Test writes exactly at and near header boundary
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    let mut graph_file = GraphFile::create(path).unwrap();

    // Write at offset 87 (last byte of header) - should fail
    let result = graph_file.write_bytes(87, b"x");
    assert!(result.is_err(), "Write to last header byte should fail");

    // Write at offset 88 (first byte after header) - should succeed
    let result = graph_file.write_bytes(88, b"safe_write");
    assert!(result.is_ok(), "Write after header region should succeed");

    // Write at offset 0 (first byte of header) - should fail
    let result = graph_file.write_bytes(0, b"header_overwrite");
    assert!(result.is_err(), "Write to first header byte should fail");
}

#[test]
fn test_multiple_header_region_rejections() {
    // Test various header region positions are all rejected
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    let mut graph_file = GraphFile::create(path).unwrap();

    let header_region_offsets = [0, 1, 42, 86, 87]; // Various offsets within 0-87
    let test_data = b"x";

    for offset in header_region_offsets.iter() {
        let result = graph_file.write_bytes(*offset, test_data);
        assert!(
            result.is_err(),
            "Write to header region offset {} should have failed",
            offset
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("attempted write into header region"));
        assert!(error_msg.contains(&format!("offset={}", offset)));
    }
}

#[test]
fn test_large_write_spanning_header_boundary() {
    // Test that large writes spanning header boundary are rejected
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    let mut graph_file = GraphFile::create(path).unwrap();

    // Write that starts before header but extends beyond it
    let large_data = vec![0u8; 100]; // 100 bytes
    let result = graph_file.write_bytes(50, &large_data);

    // Should fail because it starts in header region
    assert!(
        result.is_err(),
        "Large write starting in header region should fail"
    );
    let error_msg = format!("{}", result.unwrap_err());
    assert!(error_msg.contains("attempted write into header region"));
}

#[test]
fn test_magic_hex_output_before_after() {
    // Test to capture exact hex output for debugging
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    // Write initial header and capture hex
    let magic_before;
    {
        let mut graph_file = GraphFile::create(&path).unwrap();
        magic_before = graph_file.header().magic;
        println!("Magic before close: {:02X?}", magic_before);
    }

    // Reopen and compare
    let mut graph_file = GraphFile::open(&path).unwrap();
    let magic_after = graph_file.header().magic;
    println!("Magic after reopen: {:02X?}", magic_after);

    assert_eq!(
        magic_before, magic_after,
        "Magic bytes changed across reopen"
    );
}
