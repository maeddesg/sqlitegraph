//! V2 node record implementation (V1 conversion removed).
//!
//! Now V2-only with no V1 conversion support:
//! - `record.rs`: core struct definition + serialization/deserialization.
//! - `metrics.rs`: adjacency analytics helpers.

mod metrics;
mod record;

pub use metrics::AdjacencyMetrics;
pub use record::{NodeRecordV2, NodeRecordV2Ext, parse_v2_header_lengths};

#[cfg(all(test, feature = "v2_experimental"))]
mod tests {
    use super::*;
    use crate::backend::native::NodeRecord;

    #[test]
    fn test_v2_node_serialization() {
        let data = serde_json::json!({"key": "value"});
        let node = NodeRecordV2::new(42, "Function".to_string(), "test_func".to_string(), data);
        let serialized = node.serialize();
        assert!(serialized.len() < 200);
        assert!(serialized.len() > 50);

        let deserialized = NodeRecordV2::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.id, node.id);
        assert_eq!(deserialized.kind, node.kind);
        assert_eq!(deserialized.name, node.name);
        assert_eq!(deserialized.data, node.data);
    }

    #[test]
    fn test_v2_adjacency_metadata() {
        let mut node = NodeRecordV2::new(
            1,
            "Node".to_string(),
            "test".to_string(),
            serde_json::json!({}),
        );
        node.set_outgoing_cluster(10000, 500, 10);
        node.set_incoming_cluster(10500, 300, 5);

        assert!(node.has_outgoing_edges());
        assert!(node.has_incoming_edges());
        assert_eq!(node.total_edge_count(), 15);
        assert_eq!(node.outgoing_cluster_offset, 10000);
        assert_eq!(node.incoming_cluster_size, 300);
    }

    #[test]
    fn test_adjacency_efficiency() {
        let mut node = NodeRecordV2::new(
            1,
            "Node".to_string(),
            "test".to_string(),
            serde_json::json!({}),
        );

        node.set_outgoing_cluster(10000, 500, 10);
        node.set_incoming_cluster(10500, 200, 4);
        assert!(node.has_efficient_adjacency());

        node.set_outgoing_cluster(10000, 50, 10);
        assert!(!node.has_efficient_adjacency());
    }

    #[test]
    fn test_node_validation() {
        let mut node = NodeRecordV2::new(
            1,
            "Node".to_string(),
            "test".to_string(),
            serde_json::json!({}),
        );
        assert!(node.validate().is_ok());

        node.outgoing_edge_count = 5;
        assert!(node.validate().is_err());

        node.outgoing_cluster_offset = 1000;
        assert!(node.validate().is_err());

        node.outgoing_cluster_size = 250;
        assert!(node.validate().is_ok());
    }

    #[test]
    fn test_cluster_size_estimation() {
        assert_eq!(NodeRecordV2::estimate_cluster_size(0), 0);
        assert_eq!(NodeRecordV2::estimate_cluster_size(1), 58);
        assert_eq!(NodeRecordV2::estimate_cluster_size(10), 508);
    }
}
