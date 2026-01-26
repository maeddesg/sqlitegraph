//! In-process pub/sub for SQLiteGraph
//!
//! Minimal, best-effort pub/sub system with:
//! - ID-only events (no payloads)
//! - Emitted on commit only
//! - In-process channel delivery
//! - No persistence or delivery guarantees
//!
//! # Architecture
//!
//! Events are emitted when transactions commit and carry only identifiers
//! (node_id, edge_id, key_hash, snapshot_id). Consumers must read actual
//! data from the graph/KV APIs using the provided snapshot_id.
//!
//! # Module Organization
//!
//! - [`event`](crate::backend::native::v2::pubsub::event): Event type definitions
//! - [`subscriber`](crate::backend::native::v2::pubsub::subscriber): Subscription management
//! - [`publisher`](crate::backend::native::v2::pubsub::publisher): Channel-based event delivery
//! - [`emit`](crate::backend::native::v2::pubsub::emit): WAL record to event conversion
//!
//! # Pub/Sub Integration Tests
//!
//! The test suite validates:
//! - Event emission on commit (not rollback)
//! - All event types (NodeChanged, EdgeChanged, KVChanged, SnapshotCommitted)
//! - Filter behavior by event type and entity IDs
//! - Multiple subscribers receive independent events
//! - Unsubscribe stops event delivery
//! - Best-effort delivery (no panic on dropped receiver)
//!
//! Run tests with: `cargo test --lib pubsub`

pub mod event;
pub mod subscriber;
pub mod publisher;
pub mod emit;

pub use event::{PubSubEvent, PubSubEventType};
pub use subscriber::{Subscriber, SubscriberId, SubscriptionFilter};
pub use publisher::Publisher;
pub use emit::{records_to_events, should_emit_event};

#[cfg(test)]
mod tests;
