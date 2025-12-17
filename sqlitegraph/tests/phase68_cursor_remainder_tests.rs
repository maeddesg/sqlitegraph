use sqlitegraph::backend::native::NativeBackendError;
use sqlitegraph::backend::native::v2::edge_cluster::{
    Direction, EdgeCluster, TraceContext, TraceGuard,
};

#[test]
fn test_cursor_remainder_matches_buffer_size() {
    // Build a single-edge cluster whose payload advertises more bytes than are available.
    let mut payload = Vec::new();
    payload.extend_from_slice(&42_i64.to_be_bytes()); // neighbor_id
    payload.extend_from_slice(&1u16.to_be_bytes()); // edge_type_offset
    payload.extend_from_slice(&(8774u16).to_be_bytes()); // corrupt edge_data_len
    payload.extend_from_slice(b"{\"edge_index\":0,\"payload\":\"ascii_remainder\"}");
    payload.extend_from_slice(b"!!"); // ensure total payload bytes == 58
    assert_eq!(payload.len(), 58);

    let mut cluster_bytes = Vec::new();
    cluster_bytes.extend_from_slice(&1u32.to_be_bytes()); // edge_count
    cluster_bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes()); // payload_size
    cluster_bytes.extend_from_slice(&payload);

    let _trace_guard = TraceGuard::new(TraceContext {
        node_id: 8,
        direction: Direction::Incoming,
        cluster_offset: 6243328,
        payload_size: payload.len() as u32,
        strict: true,
    });

    match EdgeCluster::deserialize(&cluster_bytes) {
        Err(NativeBackendError::CorruptEdgeRecord { reason, .. }) => {
            assert!(
                reason.contains("framed edge payload truncated"),
                "unexpected corruption detail: {reason}"
            );
            assert!(
                reason.contains("remaining=58"),
                "cursor remainder missing from reason: {reason}"
            );
        }
        other => panic!("expected CorruptEdgeRecord error, got {:?}", other),
    }
}
