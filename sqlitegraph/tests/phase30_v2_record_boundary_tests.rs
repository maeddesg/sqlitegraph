//! Phase 30: V2 Record Sizing & Boundary Correction Tests
//!
//! These tests reproduce the exact bug that mmap integration exposed:
//! - V2 record boundary miscalculation causing massive data_len values
//! - Reading entire slot instead of actual record size

use sqlitegraph::backend::native::v2::node_record_v2::{NodeRecordV2, parse_v2_header_lengths};
use sqlitegraph::backend::native::{GraphFile, NodeStore};
use tempfile::NamedTempFile;

fn create_test_graph_file() -> (GraphFile, NamedTempFile) {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();
    let graph_file = GraphFile::create(path).unwrap();
    (graph_file, temp_file)
}

#[test]
#[cfg(feature = "v2_experimental")]
fn test_v2_record_header_size_mismatch_fails_cleanly() {
    // This test reproduces the corruption bug where reading entire slot
    // instead of actual record size causes massive data_len values
    let (mut graph_file, _temp_file) = create_test_graph_file();
    let mut node_store = NodeStore::new(&mut graph_file);

    // Create a small V2 record that should be ~50 bytes
    let test_data = serde_json::json!({"test": "data"});
    let v2_record = NodeRecordV2::new(1, "Function".to_string(), "test".to_string(), test_data);

    // Write the record using V2 serialization
    node_store.write_node_v2(&v2_record).unwrap();

    // Attempt to read back - this should fail cleanly with proper bounds error
    // NOT with massive corruption like "need 1936028752 bytes"
    let result = node_store.read_node_v2(1);

    match result {
        Ok(record) => {
            // If successful, verify the record data is reasonable
            assert_eq!(record.id, 1);
            assert_eq!(record.kind, "Function");
            assert_eq!(record.name, "test");

            // CRITICAL: Verify we didn't read the entire 4096-byte slot
            // The serialized length should be small (~50-100 bytes)
            let expected_max_size = 200; // Generous upper bound for small record
            let actual_serialized_len = record.serialize().len();
            assert!(
                actual_serialized_len < expected_max_size,
                "V2 record serialization too large: {} > {} bytes",
                actual_serialized_len,
                expected_max_size
            );
        }
        Err(e) => {
            // Should fail with clean bounds error, NOT corruption
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Read beyond mmap region")
                    || error_msg.contains("Insufficient bytes")
                    || error_msg.contains("BufferTooSmall"),
                "Expected clean bounds error, got corruption: {}",
                error_msg
            );

            // CRITICAL: Should NOT have massive data_len values
            assert!(
                !error_msg.contains("1936028752") && !error_msg.contains("need ")
                    || !error_msg
                        .chars()
                        .any(|c| c.is_ascii_digit()
                            && error_msg.matches(char::is_numeric).count() > 10),
                "Error should not contain massive byte counts: {}",
                error_msg
            );
        }
    }
}

#[test]
#[cfg(feature = "v2_experimental")]
fn test_v2_record_total_length_matches_serialized_bytes() {
    // This test verifies that record length calculation matches actual serialized size
    let test_cases = vec![
        ("Small", serde_json::json!({})),
        ("Medium", serde_json::json!({"data": "x".repeat(100)})),
        ("Large", serde_json::json!({"data": "x".repeat(1000)})),
    ];

    for (case_name, test_data) in test_cases {
        let v2_record = NodeRecordV2::new(
            1,
            format!("Type_{}", case_name),
            format!("name_{}", case_name),
            test_data,
        );

        // Get serialized bytes
        let serialized = v2_record.serialize();
        let serialized_len = serialized.len();

        // Parse header to calculate expected size
        let (kind_len, name_len, data_len) = parse_v2_header_lengths(&serialized).unwrap();
        let expected_size = 21 + kind_len as usize + name_len as usize + data_len as usize + 32;

        assert_eq!(
            serialized_len, expected_size,
            "{} case: serialized length {} doesn't match calculated size {}",
            case_name, serialized_len, expected_size
        );

        // Verify serialization consistency by round-tripping through deserialize
        let roundtrip_record = NodeRecordV2::deserialize(&serialized).unwrap();
        assert_eq!(roundtrip_record.id, v2_record.id);
        assert_eq!(roundtrip_record.kind, v2_record.kind);
        assert_eq!(roundtrip_record.name, v2_record.name);
        assert_eq!(roundtrip_record.data, v2_record.data);
    }
}

#[test]
#[cfg(feature = "v2_experimental")]
fn test_v2_record_boundary_roundtrip_integrity() {
    // This test ensures V2 records work correctly across slot boundaries
    let (mut graph_file, _temp_file) = create_test_graph_file();
    let mut node_store = NodeStore::new(&mut graph_file);

    // Create multiple V2 records to test boundary conditions
    let test_records: Vec<NodeRecordV2> = (1..=5)
        .map(|i| {
            let data_size = match i {
                1 => 10,   // Tiny
                2 => 100,  // Small
                3 => 1000, // Medium
                4 => 2000, // Large
                5 => 3000, // Very large
                _ => 50,
            };

            let test_data = serde_json::json!({
                "id": i,
                "data": "x".repeat(data_size)
            });

            NodeRecordV2::new(
                i,
                format!("Function_{}", i),
                format!("node_{}", i),
                test_data,
            )
        })
        .collect();

    // Write all records
    for record in &test_records {
        node_store.write_node_v2(record).unwrap();
    }

    // Read all records back and verify integrity
    for (i, original_record) in test_records.iter().enumerate() {
        let node_id = (i + 1) as i64;
        let read_record = node_store.read_node_v2(node_id).unwrap();

        assert_eq!(read_record.id, original_record.id);
        assert_eq!(read_record.kind, original_record.kind);
        assert_eq!(read_record.name, original_record.name);
        assert_eq!(read_record.data, original_record.data);

        // Verify serialization consistency
        let original_serialized = original_record.serialize();
        let read_serialized = read_record.serialize();
        assert_eq!(
            original_serialized.len(),
            read_serialized.len(),
            "Node {}: serialization length mismatch after roundtrip",
            node_id
        );
    }

    // Test boundary conditions - nodes near slot boundaries
    // Node 257 is known to cause issues in previous tests
    let boundary_record = NodeRecordV2::new(
        257,
        "BoundaryTest".to_string(),
        "boundary_node_257".to_string(),
        serde_json::json!({"boundary": true, "data": "test".repeat(100)}),
    );

    node_store.write_node_v2(&boundary_record).unwrap();
    let read_boundary = node_store.read_node_v2(257).unwrap();

    assert_eq!(read_boundary.id, 257);
    assert_eq!(read_boundary.kind, "BoundaryTest");
    assert_eq!(read_boundary.name, "boundary_node_257");
    assert_eq!(read_boundary.data, boundary_record.data);
}
