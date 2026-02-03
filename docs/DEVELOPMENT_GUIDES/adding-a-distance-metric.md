# Adding a Distance Metric to HNSW

**Last Updated:** 2026-02-03
**Version:** v1.4.1

This guide explains how to add a new distance metric for HNSW vector search.

---

## Overview

HNSW distance metrics are located in `src/hnsw/distance.rs`. Each metric:
- Implements the `DistanceMetric` trait
- Provides SIMD-optimized computation where possible
- Supports various dimension sizes

---

## Step-by-Step Guide

### Step 1: Add Metric Enum Variant

Update `src/hnsw/distance.rs`:

```rust
/// Distance metric for vector comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
    Manhattan,
    YourMetric,  // Add this
}
```

### Step 2: Implement Distance Calculation

Add implementation in `src/hnsw/distance.rs`:

```rust
impl DistanceMetric {
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

    /// Get the default configuration for this metric
    pub fn default_config(&self) -> HnswConfig {
        match self {
            DistanceMetric::Cosine => HnswConfig::default(),
            DistanceMetric::Euclidean => HnswConfig::default(),
            DistanceMetric::DotProduct => HnswConfig::default(),
            DistanceMetric::Manhattan => HnswConfig::default(),
            DistanceMetric::YourMetric => {
                HnswConfig::builder()
                    .distance_metric(DistanceMetric::YourMetric)
                    .build()
                    .unwrap()
            }
        }
    }
}

/// Your custom distance metric
///
/// Description of what this metric computes and when to use it.
fn your_metric_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    // Your distance calculation here
    // Example: Chebyshev distance (L-infinity norm)
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

/// SIMD-optimized version (optional, for AVX2)
#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn your_metric_distance_simd(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;

    // SIMD implementation for better performance
    // This is optional but recommended for common operations
    your_metric_distance_fallback(a, b)
}

/// Fallback for non-SIMD or remaining elements
#[inline]
fn your_metric_distance_fallback(a: &[f32], b: &[f32]) -> f32 {
    your_metric_distance(a, b)
}
```

### Step 3: Add Tests

Add to `src/hnsw/distance.rs` tests module:

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
    fn test_your_metric_via_enum() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let metric = DistanceMetric::YourMetric;
        let dist = metric.compute(&a, &b);
        assert_eq!(dist, 0.0);
    }
}
```

### Step 4: Add Integration Tests

Add to `tests/hnsw_tests.rs`:

```rust
#[test]
fn test_hnsw_with_your_metric() {
    use sqlitegraph::hnsw::{HnswConfig, DistanceMetric};

    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");

    let config = HnswConfig::builder()
        .dimension(128)
        .distance_metric(DistanceMetric::YourMetric)
        .build()
        .unwrap();

    let mut hnsw = HnswIndex::new_with_db(&db_path.to_string_lossy(), config).unwrap();

    // Insert vectors
    for i in 0..10 {
        let vector: Vec<f32> = (0..128).map(|j| (i * 128 + j) as f32).collect();
        hnsw.insert_vector(&vector, None).unwrap();
    }

    // Search
    let query: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let results = hnsw.search(&query, 5).unwrap();

    assert!(!results.is_empty());
}
```

### Step 5: Update CLI

Update `sqlitegraph-cli/src/main.rs` to support your metric:

```rust
// In the HNSW create command parsing
let distance_metric = match args.distance_metric.as_str() {
    "cosine" => DistanceMetric::Cosine,
    "euclidean" => DistanceMetric::Euclidean,
    "dotproduct" => DistanceMetric::DotProduct,
    "manhattan" => DistanceMetric::Manhattan,
    "yourmetric" => DistanceMetric::YourMetric,  // Add this
    _ => return Err(anyhow!("Unknown distance metric: {}", args.distance_metric)),
};
```

### Step 6: Update Documentation

Add to `MANUAL.md` Section 8 (HNSW Vector Search):

```markdown
### Distance Metrics

| Metric | Best For | Speed |
|--------|----------|-------|
| **Cosine** | Text embeddings | Fast |
| **Euclidean** | General similarity | Medium |
| **Dot Product** | Normalized vectors | Fastest |
| **Manhattan** | Sparse vectors | Slow |
| **YourMetric** | [Description] | [Speed] |

#### YourMetric

[Explain what your metric does and when to use it]

Mathematical definition: d(a, b) = [formula]

Properties:
- Range: [min, max]
- Metric space: [yes/no]
- Use cases: [when to use]
```

---

## Distance Metric Guidelines

### DO:

1. **Implement metric properties**:
   - Non-negativity: d(a, b) >= 0
   - Identity: d(a, a) = 0
   - Symmetry: d(a, b) = d(b, a)
   - Triangle inequality: d(a, c) <= d(a, b) + d(b, c)

2. **Use SIMD optimization** for common dimensions (128, 256, 1536)

3. **Handle edge cases**: empty vectors, NaN, infinity

4. **Document use cases**: When is this metric appropriate?

### DON'T:

1. **Assume vector length**: Always validate or handle variable lengths

2. **Use unsafe without reason**: SIMD should be optional

3. **Break metric properties**: Unless you have a specific reason

---

## Example: Chebyshev Distance

For a complete example, here's adding Chebyshev (L-infinity) distance:

```rust
/// Chebyshev distance (L-infinity norm)
/// d(a, b) = max(|a_i - b_i|)
fn chebyshev_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_chebyshev() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 0.0, 3.0];
        // max(|1-4|, |2-0|, |3-3|) = max(3, 2, 0) = 3
        assert_eq!(chebyshev_distance(&a, &b), 3.0);
    }
}
```

---

## Testing Checklist

- [ ] Unit tests for metric properties
- [ ] Integration test with HNSW index
- [ ] SIMD tests (if applicable)
- [ ] Edge case tests (empty, NaN, inf)
- [ ] CLI support added
- [ ] Documentation updated

---

## Common Issues

### Issue: Metric violates triangle inequality

**Solution:** HNSW doesn't strictly require metric properties, but performance may degrade. Consider if a different index structure would be better.

### Issue: Results are different from expected

**Solution:** Verify your implementation matches the mathematical definition. Test with known inputs/outputs.

### Issue: Performance is poor

**Solution:** Add SIMD optimization for common dimension sizes. Profile to find bottlenecks.
