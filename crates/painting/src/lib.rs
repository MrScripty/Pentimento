//! Pentimento painting system - stroke and dab data structures
//!
//! This crate provides the core data types for the painting system:
//! - [`types::StrokePacket`] - A complete stroke with header and dabs
//! - [`types::Dab`] - A single brush dab (GPU-compatible with bytemuck)
//! - [`validation`] - Helpers for coordinate conversion and validation

pub mod constants;
pub mod types;
pub mod validation;

pub use constants::*;
pub use types::*;
pub use validation::*;
