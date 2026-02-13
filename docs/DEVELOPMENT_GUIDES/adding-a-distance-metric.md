# Adding a Distance Metric to HNSW

**Last Updated:** 2026-02-12  
**Version:** v1.6.0

This guide explains how to add a new distance metric for HNSW vector search.

---

## Overview

HNSW distance metrics are defined in:
- `src/hnsw/distance_metric.rs` - Enum definition
- `src/hnsw/distance_functions.rs` - Function implementations

Each metric:
- Implements the `DistanceMetric` trait
- Provides SIMD-optimized computation where possible
- Supports various dimension sizes

---

## Current Distance Metrics

| Metric | Location | Description |
|--------|----------|-------------|
| Cosine | `distance_functions.rs` | Angle between vectors (most common for text) |
| Euclidean | `distance_functions.rs` | L2 norm, straight-line distance |
| DotProduct | `distance_functions.rs` | Negative dot product (for normalized vectors) |
| Manhattan | `distance_functions.rs` | L1 norm, sum of absolute differences |

---

## Step-by-Step Guide

### Step 1: Add Metric Enum Variant

Update `src/hnsw/distance_metric.rs`:

```rust
/// Distance metric for vector comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DistanceMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
    Manhattan,
    #[serde(rename = "your_metric")]
    YourMetric,  // Add this
}
```

### Step 2: Implement Distance Calculation

Add to `src/hnsw/distance_functions.rs`:

```rust
/// Your custom distance metric
///
/// Description of what this metric computes and when to use it.
pub fn your_metric_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    // Your distance calculation here
    // Example: Chebyshev distance (L-infinity norm)
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

/// SIMD-optimized version (optional, for AVX2)
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn your_metric_distance_simd(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    
    // See cosine_distance_simd for reference implementation
    // This is optional but recommended for common operations
    your_metric_distance_fallback(a, b)
}

/// Fallback for non-SIMD or remaining elements
#[inline]
fn your_metric_distance_fallback(a: &[f32], b: &[f32]) -> f32 {
    your_metric_distance(a, b)
}
```

### Step 3: Wire into Trait Implementation

Update the trait implementation in `distance_metric.rs`:

```rust
impl DistanceMetric {
    /// Get the string name for this metric
    pub fn as_str(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => "cosine",
            DistanceMetric::Euclidean => "euclidean",
            DistanceMetric::DotProduct => "dot_product",
            DistanceMetric::Manhattan => "manhattan",
            DistanceMetric::YourMetric => "your_metric",
        }
    }

    /// Calculate distance between two vectors
    pub fn compute(&self, a: &[f32], b: &[f32]) -> f32 {
        match self {
            DistanceMetric::Cosine => cosine_distance(a, b),
            DistanceMetric::Euclidean => euclidean_distance(a, b),
            DistanceMetric::DotProduct => dot_product_distance(a, b),
            DistanceMetric::Manhattan => manhattan_distance(a, b),
            DistanceMetric::YourMetric => your_metric_distance(a, b),
        }
    }

    /// Check if this metric is normalized (produces values in [0, 1])
    pub fn is_normalized(&self) -> bool {
        match self {
            DistanceMetric::Cosine => true,
            DistanceMetric::DotProduct => true,
            DistanceMetric::Euclidean => false,
            DistanceMetric::Manhattan => false,
            DistanceMetric::YourMetric => false, // Update based on your metric
        }
    }
}
```

### Step 4: Add Configuration Builder Support

Update `src/hnsw/config.rs` if needed:

```rust
impl HnswConfigBuilder {
    /// Set the distance metric (default: Cosine)
    pub fn distance_metric(mut self, metric: DistanceMetric) -> Self {
        self.distance_metric = metric;
        self
    }
}
```

### Step 5: Add Tests

Add to `src/hnsw/distance_functions.rs` tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_your_metric_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let dist = your_metric_distance(&a, &b);
        assert_eq!(dist, 0.0);
    }

    #[test]
    fn test_your_metric_different_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let dist = your_metric_distance(&a, &b);
        assert!(dist > 0.0);
    }

    #[test]
    fn test_your_metric_symmetry() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let dist_ab = your_metric_distance(&a, &b);
        let dist_ba = your_metric_distance(&b, &a);
        assert!((dist_ab - dist_ba).abs() < 1e-6);
    }

    #[test]
    fn test_your_metric_triangle_inequality() {
        // d(a, c) <= d(a, b) + d(b, c)
        let a = vec![1.0, 2.0];
        let b = vec![3.0, 4.0];
        let c = vec![5.0, 6.0];

        let dist_ac = your_metric_distance(&a, &c);
        let dist_ab = your_metric_distance(&a, &b);
        let dist_bc = your_metric_distance(&b, &c);

        assert!(dist_ac <= dist_ab + dist_bc + 1e-6);
    }

    #[test]
    fn test_your_metric_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        // Should panic
        std::panic::catch_unwind(|| {
            your_metric_distance(&a, &b);
        }).unwrap_err();
    }
}
```

### Step 6: Integration Test

Create a test using your metric with a real HNSW index:

```rust
#[test]
fn test_your_metric_with_index() {
    use crate::hnsw::{HnswIndex, HnswConfig, DistanceMetric};
    
    let config = HnswConfig::builder()
        .dimension(128)
        .distance_metric(DistanceMetric::YourMetric)
        .build()
        .unwrap();
    
    let mut index = HnswIndex::new("test", config).unwrap();
    
    // Insert test vectors
    for i in 0..10 {
        let vec: Vec<f32> = (0..128).map(|j| (i * j) as f32 / 1000.0).collect();
        index.insert(vec, None).unwrap();
    }
    
    // Search
    let query: Vec<f32> = (0..128).map(|j| j as f32 / 1000.0).collect();
    let results = index.search(&query, 3).unwrap();
    
    assert_eq!(results.len(), 3);
}
```

---

## Complete Example: Adding Chebyshev Distance

Here's a complete example for Chebyshev distance (L-infinity norm):

```rust
// In distance_metric.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DistanceMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
    Manhattan,
    #[serde(rename = "chebyshev")]
    Chebyshev,
}

impl DistanceMetric {
    pub fn as_str(&self) -> &'static str {
        match self {
            // ... other variants
            DistanceMetric::Chebyshev => "chebyshev",
        }
    }

    pub fn compute(&self, a: &[f32], b: &[f32]) -> f32 {
        match self {
            // ... other variants
            DistanceMetric::Chebyshev => chebyshev_distance(a, b),
        }
    }

    pub fn is_normalized(&self) -> bool {
        match self {
            // ... other variants
            DistanceMetric::Chebyshev => false,
        }
    }
}

// In distance_functions.rs
/// Chebyshev distance (L-infinity norm)
/// Maximum absolute difference across all dimensions
/// Useful for scenarios where only the largest difference matters
pub fn chebyshev_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");
    
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chebyshev_distance() {
        // d([1,2,3], [4,1,5]) = max(|1-4|, |2-1|, |3-5|) = max(3, 1, 2) = 3
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 1.0, 5.0];
        assert_eq!(chebyshev_distance(&a, &b), 3.0);
    }

    #[test]
    fn test_chebyshev_identical() {
        let a = vec![1.0, 2.0, 3.0];
        assert_eq!(chebyshev_distance(&a, &a), 0.0);
    }
}
```

---

## Testing Your New Metric

```bash
# Run distance function tests
cargo test --lib hnsw::distance_functions::tests

# Run HNSW tests with your metric
cargo test --lib hnsw::tests

# Run integration tests
cargo test --features native-v3 --lib hnsw
```

---

## Best Practices

1. **Always assert dimension equality** - Prevents subtle bugs
2. **Handle edge cases** - Empty vectors, NaN, infinity
3. **Consider SIMD** - For large vectors (256+ dimensions)
4. **Document assumptions** - Normalization requirements, value ranges
5. **Test symmetry** - d(a,b) should equal d(b,a)
6. **Test triangle inequality** - Required for metric spaces
7. **Benchmark** - Compare performance with existing metrics

---

## See Also

- [HNSW Internals](hnsw-internals.md)
- [Testing Guide](testing.md)
- Source: `src/hnsw/distance_metric.rs`
- Source: `src/hnsw/distance_functions.rs`
