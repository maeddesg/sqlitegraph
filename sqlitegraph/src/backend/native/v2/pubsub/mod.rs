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

pub mod event;
pub mod subscriber;

pub use event::{PubSubEvent, PubSubEventType};
pub use subscriber::{Subscriber, SubscriberId, SubscriptionFilter};
