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
//! - [`projection`] - Brush projection math for 3D mesh painting
//! - [`half_edge`] - Half-edge mesh data structure for mesh editing

pub mod brush;
pub mod constants;
#[cfg(feature = "bevy")]
pub mod half_edge;
pub mod log;
pub mod mesh_surface;
pub mod pipeline;
pub mod projection;
pub mod projection_target;
pub mod raycast;
pub mod surface;
pub mod tiles;
pub mod types;
pub mod validation;

pub use brush::*;
pub use constants::*;
#[cfg(feature = "bevy")]
pub use half_edge::*;
pub use log::*;
pub use mesh_surface::*;
pub use pipeline::*;
pub use projection::*;
pub use projection_target::*;
pub use raycast::*;
pub use surface::*;
pub use tiles::*;
pub use types::*;
pub use validation::*;
