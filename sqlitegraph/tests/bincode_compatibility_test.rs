//! Bincode 2.0 compatibility tests
//!
//! This test verifies that serialization works correctly with bincode 2.0.

/// A simple test struct for serialization testing
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct TestStruct {
    id: u64,
    name: String,
    value: i32,
}

#[test]
fn test_bincode_encode() -> Result<(), Box<dyn std::error::Error>> {
    let data = TestStruct {
        id: 42,
        name: "test".to_string(),
        value: -100,
    };

    // Test that bincode 2.0 encode works with serde feature
    let encoded = bincode::serde::encode_to_vec(&data, bincode::config::standard())?;

    assert!(!encoded.is_empty());
    // Small struct with id (8 bytes), len (varint), string "test" (4 bytes + 1 byte len),
    // and value (4 bytes) should be at least a few bytes
    assert!(encoded.len() >= 5);

    Ok(())
}

#[test]
fn test_bincode_decode() -> Result<(), Box<dyn std::error::Error>> {
    let data = TestStruct {
        id: 42,
        name: "test".to_string(),
        value: -100,
    };

    let encoded = bincode::serde::encode_to_vec(&data, bincode::config::standard())?;

    // Test that bincode 2.0 decode works with serde feature
    // Note: decode_from_slice returns (value, bytes_read)
    let (decoded, _bytes_read): (TestStruct, usize) =
        bincode::serde::decode_from_slice(&encoded, bincode::config::standard())?;

    assert_eq!(data, decoded);

    Ok(())
}

#[test]
fn test_bincode_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let original = TestStruct {
        id: 123456789012345678,
        name: "Hello, bincode 2.0!".to_string(),
        value: 42,
    };

    // Encode
    let encoded = bincode::serde::encode_to_vec(&original, bincode::config::standard())?;

    // Decode
    let (decoded, _): (TestStruct, usize) =
        bincode::serde::decode_from_slice(&encoded, bincode::config::standard())?;

    // Verify round-trip
    assert_eq!(original, decoded);
    assert_eq!(decoded.id, 123456789012345678);
    assert_eq!(decoded.name, "Hello, bincode 2.0!");
    assert_eq!(decoded.value, 42);

    Ok(())
}

#[test]
fn test_bincode_vec_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let original = vec![
        TestStruct { id: 1, name: "first".to_string(), value: 10 },
        TestStruct { id: 2, name: "second".to_string(), value: 20 },
        TestStruct { id: 3, name: "third".to_string(), value: 30 },
    ];

    let encoded = bincode::serde::encode_to_vec(&original, bincode::config::standard())?;

    let (decoded, _): (Vec<TestStruct>, usize) =
        bincode::serde::decode_from_slice(&encoded, bincode::config::standard())?;

    assert_eq!(original, decoded);
    assert_eq!(decoded.len(), 3);

    Ok(())
}

#[test]
fn test_bincode_basic_types() -> Result<(), Box<dyn std::error::Error>> {
    // Test basic types round-trip
    let original_i32: i32 = -12345;
    let encoded_i32 = bincode::serde::encode_to_vec(&original_i32, bincode::config::standard())?;
    let (decoded_i32, _): (i32, usize) =
        bincode::serde::decode_from_slice(&encoded_i32, bincode::config::standard())?;
    assert_eq!(original_i32, decoded_i32);

    let original_u64: u64 = 987654321012345678;
    let encoded_u64 = bincode::serde::encode_to_vec(&original_u64, bincode::config::standard())?;
    let (decoded_u64, _): (u64, usize) =
        bincode::serde::decode_from_slice(&encoded_u64, bincode::config::standard())?;
    assert_eq!(original_u64, decoded_u64);

    let original_str: String = "Hello, bincode 2.0 with serde!".to_string();
    let encoded_str = bincode::serde::encode_to_vec(&original_str, bincode::config::standard())?;
    let (decoded_str, _): (String, usize) =
        bincode::serde::decode_from_slice(&encoded_str, bincode::config::standard())?;
    assert_eq!(original_str, decoded_str);

    Ok(())
}

#[test]
fn test_bincode_format_size() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that the serialization format is reasonable
    let data = TestStruct {
        id: 1,
        name: "a".to_string(),
        value: 100,
    };

    let encoded = bincode::serde::encode_to_vec(&data, bincode::config::standard())?;

    // Verify we get a reasonable encoding size
    // bincode should encode this efficiently, certainly less than 100 bytes
    assert!(encoded.len() < 100, "Encoded size {} is unexpectedly large", encoded.len());

    // Should be non-zero
    assert!(encoded.len() > 0);

    Ok(())
}
