//! Edge clustering primitives for the V2 storage kernel.
//!
//! This module is intentionally split into small components to keep each file
//! below the 300‑line architectural limit. Public consumers should import
//! items from this `mod.rs` re-export rather than submodules directly.

mod cluster;
mod compact_record;
mod record_ext;

pub use cluster::{Direction, EdgeCluster, StrictModeGuard, TraceContext, TraceGuard};
pub use compact_record::CompactEdgeRecord;
pub use record_ext::EdgeRecordCompactExt;
