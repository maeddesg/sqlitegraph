//! Storage media detection and adaptive configuration
//!
//! Automatically detects storage media type (SSD vs HDD) and configures
//! optimal page sizes and I/O strategies for improved performance.

pub mod media_detector;
pub mod adaptive_page;

pub use media_detector::{MediaDetector, MediaType};
pub use adaptive_page::{AdaptivePageManager, PageConfig};
