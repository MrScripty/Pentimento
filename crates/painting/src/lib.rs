//! Pentimento painting system - stroke and dab data structures
//!
//! This crate provides the core data types for the painting system:
//! - [`types::StrokePacket`] - A complete stroke with header and dabs
//! - [`types::Dab`] - A single brush dab (GPU-compatible with bytemuck)
//! - [`validation`] - Helpers for coordinate conversion and validation
//! - [`surface`] - CPU 16-bit RGBA surface for painting
//! - [`tiles`] - Tile management with dirty tracking
//! - [`log`] - Stroke log storage and Iroh-ready hooks
//! - [`brush`] - Brush engine for dab generation
//! - [`pipeline`] - Complete painting pipeline

pub mod brush;
pub mod constants;
pub mod log;
pub mod pipeline;
pub mod surface;
pub mod tiles;
pub mod types;
pub mod validation;

pub use brush::*;
pub use constants::*;
pub use log::*;
pub use pipeline::*;
pub use surface::*;
pub use tiles::*;
pub use types::*;
pub use validation::*;
