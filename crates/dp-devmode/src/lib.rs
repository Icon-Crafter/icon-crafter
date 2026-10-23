//! Developer handoff (Dev Mode) for Icon Crafter.
//!
//! Three responsibilities live here:
//! * [`inspect`] resolves measurements, paints, typography and component
//!   references for any selection;
//! * [`codegen`] turns an inspected selection into deterministic CSS, React +
//!   TypeScript, SwiftUI or Jetpack Compose snippets with explicit caveats;
//! * [`workbench`] tracks implementation annotations, readiness status and
//!   structural revision diffs.
//!
//! All functions are pure: they read a [`dp_model::Document`] plus its layout
//! and never mutate the design.

pub mod codegen;
pub mod inspect;
pub mod workbench;

pub use codegen::{CodeTarget, CodegenError, CodegenOptions, GeneratedCode, generate};
pub use inspect::{
    ComponentInfo, EffectInfo, GradientStopInfo, InspectSnapshot, NodeInspect, PaintInfo,
    StrokeInfo, TextInfo, document_mode, inspect, inspect_node,
};
pub use workbench::{
    Annotation, AnnotationKind, DiffCounts, Handoff, HandoffStatus, NodeChange, NodeDiff,
    RevisionDiff, compare_documents,
};

/// Version of the Dev Mode API surface.
pub const DEVMODE_VERSION: &str = "0.1.0";
