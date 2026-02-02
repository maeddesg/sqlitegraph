use sqlitegraph::{SqliteGraph, GraphEntity, GraphEdge};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Phase 45: Core Graph Theory algorithms...\n");
    
    // Create a simple test graph: 0 -> 1 -> 2 -> 3
    let graph = SqliteGraph::open_in_memory()?;
    
    // Insert nodes
    let mut node_ids = Vec::new();
    for i in 0..4 {
        let entity = GraphEntity {
            id: 0,
            kind: "node".to_string(),
            name: format!("node_{}", i),
            file_path: Some(format!("node_{}.rs", i)),
            data: serde_json::json!({"index": i}),
        };
        let id = graph.insert_entity(&entity)?;
        node_ids.push(id);
    }
    
    // Insert edges: 0 -> 1 -> 2 -> 3
    for i in 0..node_ids.len().saturating_sub(1) {
        let edge = GraphEdge {
            id: 0,
            from_id: node_ids[i],
            to_id: node_ids[i + 1],
            edge_type: "next".to_string(),
            data: serde_json::json!({}),
        };
        graph.insert_edge(&edge)?;
    }
    
    println!("Created graph with {} nodes", node_ids.len());
    
    // Test 1: Weakly Connected Components
    println!("\n=== Test 1: Weakly Connected Components ===");
    let wcc = sqlitegraph::algo::weakly_connected_components(&graph)?;
    println!("✓ WCC found {} component(s)", wcc.len());
    assert_eq!(wcc.len(), 1, "Should have 1 component");
    
    // Test 2: Strongly Connected Components (Tarjan's)
    println!("\n=== Test 2: Strongly Connected Components (Tarjan's) ===");
    let scc = sqlitegraph::algo::strongly_connected_components(&graph)?;
    println!("✓ SCC found {} component(s)", scc.components.len());
    println!("  - Non-trivial SCCs (cycles): {}", scc.non_trivial_count());
    println!("  - Condensed DAG edges: {}", scc.condensed_edges.len());
    assert_eq!(scc.components.len(), 4, "Linear chain has 4 trivial SCCs");
    assert_eq!(scc.non_trivial_count(), 0, "No cycles in linear chain");
    
    // Test 3: Topological Sort
    println!("\n=== Test 3: Topological Sort ===");
    match sqlitegraph::algo::topological_sort(&graph) {
        Ok(ordering) => {
            println!("✓ Topological sort succeeded with {} nodes", ordering.len());
        }
        Err(e) => {
            println!("✗ Topological sort failed: {}", e);
            return Err(e.into());
        }
    }
    
    // Test 4: Transitive Closure
    println!("\n=== Test 4: Transitive Closure ===");
    let closure = sqlitegraph::algo::transitive_closure(&graph, None)?;
    println!("✓ Transitive closure computed {} reachable pairs", closure.len());
    
    // Test with bounds
    let bounds = sqlitegraph::algo::TransitiveClosureBounds {
        max_depth: Some(2),
        max_sources: None,
        max_pairs: None,
    };
    let bounded_closure = sqlitegraph::algo::transitive_closure(&graph, Some(bounds))?;
    println!("✓ Bounded closure (depth=2) computed {} pairs", bounded_closure.len());
    
    // Test 5: Transitive Reduction
    println!("\n=== Test 5: Transitive Reduction ===");
    let reduction = sqlitegraph::algo::transitive_reduction(&graph)?;
    println!("✓ Transitive reduction found {} essential edges", reduction.len());
    
    // Add a redundant edge and test again
    let redundant_edge = GraphEdge {
        id: 0,
        from_id: node_ids[0],
        to_id: node_ids[2],  // 0 -> 2 (redundant since 0 -> 1 -> 2)
        edge_type: "skip".to_string(),
        data: serde_json::json!({}),
    };
    graph.insert_edge(&redundant_edge)?;
    
    let reduction_after = sqlitegraph::algo::transitive_reduction(&graph)?;
    println!("✓ After adding redundant edge: {} essential edges", reduction_after.len());
    
    // Test with cycle for error handling
    println!("\n=== Test 6: Cycle Detection ===");
    let cycle_graph = SqliteGraph::open_in_memory()?;
    
    // Create cycle: 0 -> 1 -> 2 -> 0
    let mut cycle_node_ids = Vec::new();
    for i in 0..3 {
        let entity = GraphEntity {
            id: 0,
            kind: "node".to_string(),
            name: format!("cycle_{}", i),
            file_path: Some(format!("cycle_{}.rs", i)),
            data: serde_json::json!({}),
        };
        let id = cycle_graph.insert_entity(&entity)?;
        cycle_node_ids.push(id);
    }
    
    for i in 0..3 {
        let edge = GraphEdge {
            id: 0,
            from_id: cycle_node_ids[i],
            to_id: cycle_node_ids[(i + 1) % 3],
            edge_type: "cycle".to_string(),
            data: serde_json::json!({}),
        };
        cycle_graph.insert_edge(&edge)?;
    }
    
    match sqlitegraph::algo::topological_sort(&cycle_graph) {
        Ok(_) => {
            println!("✗ Should have detected cycle!");
            return Err("Should have detected cycle".into());
        }
        Err(sqlitegraph::algo::TopoError::CycleDetected { cycle, explanation }) => {
            println!("✓ Cycle detected correctly:");
            println!("  - Explanation: {}", explanation);
            println!("  - Cycle path length: {}", cycle.len());
        }
    }
    
    println!("\n=== All Tests Passed! ===");
    Ok(())
}
