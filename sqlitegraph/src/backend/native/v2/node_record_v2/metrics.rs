use super::NodeRecordV2;

/// Adjacency efficiency metrics for validating V2 clustering.
#[derive(Debug, Clone)]
pub struct AdjacencyMetrics {
    pub total_edges: u32,
    pub total_clusters: u32,
    pub avg_edges_per_cluster: f64,
    pub avg_edge_size_in_cluster: f64,
    pub cluster_utilization: f64,
}

impl NodeRecordV2 {
    pub fn adjacency_metrics(&self) -> AdjacencyMetrics {
        let total_edges = self.total_edge_count();
        let cluster_count =
            (self.outgoing_edge_count > 0) as u32 + (self.incoming_edge_count > 0) as u32;

        let avg_edges_per_cluster = if cluster_count > 0 {
            total_edges as f64 / cluster_count as f64
        } else {
            0.0
        };

        let total_cluster_size = self.outgoing_cluster_size + self.incoming_cluster_size;
        let avg_edge_size_in_cluster = if total_edges > 0 {
            total_cluster_size as f64 / total_edges as f64
        } else {
            0.0
        };

        let cluster_utilization = if total_cluster_size > 0 {
            avg_edge_size_in_cluster / 80.0
        } else {
            0.0
        };

        AdjacencyMetrics {
            total_edges,
            total_clusters: cluster_count,
            avg_edges_per_cluster,
            avg_edge_size_in_cluster,
            cluster_utilization,
        }
    }
}
