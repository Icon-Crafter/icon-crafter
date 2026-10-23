//! Geometry kernel for Icon Crafter.
//!
//! Pure geometry: affine transforms, Bezier path representation, shape
//! primitives converted to paths, boolean operations, stroke geometry, hit
//! testing, spatial indexing, snapping and bounds computation. This crate knows
//! nothing about documents, rendering or UI.

/// Version of the geometry kernel API surface.
pub const GEOM_VERSION: &str = "0.1.0";
