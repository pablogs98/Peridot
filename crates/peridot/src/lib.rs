//! Peridot:
//! Transparent Integration of new logic in legacy Wasm modules.

/// Configuration module for Peridot. Handles loading and parsing of configuration files.
pub mod conf;

/// Token bucket implementation for I/O rate limiting in Peridot.
pub mod token;

/// Context module for Peridot. Defines the PeridotContext struct and its associated methods.
pub mod context;

/// Metrics module for Peridot. Handles collection and reporting of system and application metrics.
pub mod metrics;

/// Plugin registry. Maps context names from the configuration file to the code that builds them.
pub mod plugin;

/// Helpers for reading guest linear memory, including the shared-memory fallback.
pub mod memory;

