//! Snapping and alignment guides.
//!
//! The engine calls these functions while moving or resizing a selection. They
//! return both the correction that must be applied to the moving geometry and
//! the guide lines that should be drawn as feedback. All values are in world
//! units; the caller converts its screen-space threshold (for example 8 screen
//! pixels) into world units before calling.
//!
//! Snapping is intentionally deterministic: ties are broken by distance and
//! then by candidate order, so the same input always produces the same guides.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::types::{Point, Rect};

/// Axis of a guide line or snap candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    /// Vertical line / horizontal correction.
    X,
    /// Horizontal line / vertical correction.
    Y,
}

impl Axis {
    /// The other axis.
    pub fn other(self) -> Axis {
        match self {
            Axis::X => Axis::Y,
            Axis::Y => Axis::X,
        }
    }
}

/// What produced a snap line (used for guide styling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapKind {
    /// Alignment to the document grid.
    Grid,
    /// Alignment to a user guide.
    Guide,
    /// Alignment of an edge with an object edge.
    ObjectEdge,
    /// Alignment of a centerline with an object center.
    ObjectCenter,
}

/// A user guide (a full-canvas line at a fixed coordinate).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    /// Guide orientation.
    pub axis: Axis,
    /// Position on the axis.
    pub pos: f64,
}

impl Guide {
    /// Vertical guide at `pos`.
    pub fn vertical(pos: f64) -> Guide {
        Guide { axis: Axis::X, pos }
    }

    /// Horizontal guide at `pos`.
    pub fn horizontal(pos: f64) -> Guide {
        Guide { axis: Axis::Y, pos }
    }
}

/// Grid configuration for snapping.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridSpec {
    /// Horizontal spacing.
    pub size_x: f64,
    /// Vertical spacing.
    pub size_y: f64,
    /// Grid origin (the top-left of the document page by default).
    pub origin: Point,
}

impl GridSpec {
    /// A square grid of `size` starting at the origin.
    pub fn square(size: f64) -> GridSpec {
        GridSpec {
            size_x: size,
            size_y: size,
            origin: Point::ZERO,
        }
    }
}

/// A guide line to draw while snapping.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapLine {
    /// Orientation.
    pub axis: Axis,
    /// Coordinate along the axis.
    pub pos: f64,
    /// Start coordinate along the other axis.
    pub from: f64,
    /// End coordinate along the other axis.
    pub to: f64,
    /// Origin of the snap.
    pub kind: SnapKind,
}

/// Configuration for a snapping pass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapOptions {
    /// Master switch.
    pub enabled: bool,
    /// Snap to other objects (edges and centers).
    pub snap_objects: bool,
    /// Snap to user guides.
    pub snap_guides: bool,
    /// Snap to the grid.
    pub snap_grid: bool,
    /// Maximum correction distance in world units.
    pub threshold: f64,
    /// Grid geometry; required when `snap_grid` is true.
    pub grid: Option<GridSpec>,
}

impl Default for SnapOptions {
    fn default() -> SnapOptions {
        SnapOptions {
            enabled: true,
            snap_objects: true,
            snap_guides: true,
            snap_grid: true,
            threshold: 6.0,
            grid: None,
        }
    }
}

/// Result of a snapping computation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SnapResult {
    /// Correction to apply to the moving geometry.
    pub delta: Point,
    /// Whether the X axis snapped.
    pub snapped_x: bool,
    /// Whether the Y axis snapped.
    pub snapped_y: bool,
    /// Guide lines to render.
    pub lines: Vec<SnapLine>,
}

impl SnapResult {
    /// No snapping.
    pub fn empty() -> SnapResult {
        SnapResult::default()
    }

    /// Whether at least one axis snapped.
    pub fn is_snapped(&self) -> bool {
        self.snapped_x || self.snapped_y
    }
}

struct Candidate {
    delta: f64,
    line: SnapLine,
}

/// Snaps a moving rectangle to object edges/centers, guides and the grid.
pub fn snap_rect(
    moving: Rect,
    targets: &[Rect],
    guides: &[Guide],
    options: &SnapOptions,
) -> SnapResult {
    if !options.enabled || options.threshold <= 0.0 {
        return SnapResult::empty();
    }
    let mut candidates_x: Vec<Candidate> = Vec::new();
    let mut candidates_y: Vec<Candidate> = Vec::new();

    let moving_x = [moving.left(), moving.center_x(), moving.right()];
    let moving_y = [moving.top(), moving.center_y(), moving.bottom()];

    if options.snap_objects {
        for target in targets {
            if !target.is_finite() {
                continue;
            }
            let target_x = [target.left(), target.center_x(), target.right()];
            for (t_index, tv) in target_x.iter().enumerate() {
                for (m_index, mv) in moving_x.iter().enumerate() {
                    let kind = if t_index == 1 && m_index == 1 {
                        SnapKind::ObjectCenter
                    } else {
                        SnapKind::ObjectEdge
                    };
                    candidates_x.push(Candidate {
                        delta: tv - mv,
                        line: SnapLine {
                            axis: Axis::X,
                            pos: *tv,
                            from: moving.top().min(target.top()),
                            to: moving.bottom().max(target.bottom()),
                            kind,
                        },
                    });
                }
            }
            let target_y = [target.top(), target.center_y(), target.bottom()];
            for (t_index, tv) in target_y.iter().enumerate() {
                for (m_index, mv) in moving_y.iter().enumerate() {
                    let kind = if t_index == 1 && m_index == 1 {
                        SnapKind::ObjectCenter
                    } else {
                        SnapKind::ObjectEdge
                    };
                    candidates_y.push(Candidate {
                        delta: tv - mv,
                        line: SnapLine {
                            axis: Axis::Y,
                            pos: *tv,
                            from: moving.left().min(target.left()),
                            to: moving.right().max(target.right()),
                            kind,
                        },
                    });
                }
            }
        }
    }

    if options.snap_guides {
        for guide in guides {
            if !guide.pos.is_finite() {
                continue;
            }
            match guide.axis {
                Axis::X => {
                    for mv in moving_x.iter() {
                        candidates_x.push(Candidate {
                            delta: guide.pos - mv,
                            line: SnapLine {
                                axis: Axis::X,
                                pos: guide.pos,
                                from: moving.top(),
                                to: moving.bottom(),
                                kind: SnapKind::Guide,
                            },
                        });
                    }
                }
                Axis::Y => {
                    for mv in moving_y.iter() {
                        candidates_y.push(Candidate {
                            delta: guide.pos - mv,
                            line: SnapLine {
                                axis: Axis::Y,
                                pos: guide.pos,
                                from: moving.left(),
                                to: moving.right(),
                                kind: SnapKind::Guide,
                            },
                        });
                    }
                }
            }
        }
    }

    if let Some(grid) = options.grid
        && options.snap_grid
        && grid.size_x > 1e-6
        && grid.size_y > 1e-6
    {
        for mv in moving_x.iter() {
            let nearest = snap_to_step(*mv, grid.origin.x, grid.size_x);
            candidates_x.push(Candidate {
                delta: nearest - mv,
                line: SnapLine {
                    axis: Axis::X,
                    pos: nearest,
                    from: moving.top(),
                    to: moving.bottom(),
                    kind: SnapKind::Grid,
                },
            });
        }
        for mv in moving_y.iter() {
            let nearest = snap_to_step(*mv, grid.origin.y, grid.size_y);
            candidates_y.push(Candidate {
                delta: nearest - mv,
                line: SnapLine {
                    axis: Axis::Y,
                    pos: nearest,
                    from: moving.left(),
                    to: moving.right(),
                    kind: SnapKind::Grid,
                },
            });
        }
    }

    let mut result = SnapResult::empty();
    if let Some((delta, lines)) = pick(candidates_x, options.threshold) {
        result.delta.x = delta;
        result.snapped_x = true;
        result.lines.extend(lines);
    }
    if let Some((delta, lines)) = pick(candidates_y, options.threshold) {
        result.delta.y = delta;
        result.snapped_y = true;
        result.lines.extend(lines);
    }
    result
}

/// Snaps a single point (pen tool, resize handles) to edges, guides and grid.
pub fn snap_point(
    point: Point,
    targets: &[Rect],
    guides: &[Guide],
    options: &SnapOptions,
) -> SnapResult {
    if !options.enabled || options.threshold <= 0.0 {
        return SnapResult::empty();
    }
    let mut candidates_x: Vec<Candidate> = Vec::new();
    let mut candidates_y: Vec<Candidate> = Vec::new();

    if options.snap_objects {
        for target in targets {
            if !target.is_finite() {
                continue;
            }
            for (index, tv) in [target.left(), target.center_x(), target.right()]
                .into_iter()
                .enumerate()
            {
                candidates_x.push(Candidate {
                    delta: tv - point.x,
                    line: SnapLine {
                        axis: Axis::X,
                        pos: tv,
                        from: point.y.min(target.top()),
                        to: point.y.max(target.bottom()),
                        kind: if index == 1 {
                            SnapKind::ObjectCenter
                        } else {
                            SnapKind::ObjectEdge
                        },
                    },
                });
            }
            for (index, tv) in [target.top(), target.center_y(), target.bottom()]
                .into_iter()
                .enumerate()
            {
                candidates_y.push(Candidate {
                    delta: tv - point.y,
                    line: SnapLine {
                        axis: Axis::Y,
                        pos: tv,
                        from: point.x.min(target.left()),
                        to: point.x.max(target.right()),
                        kind: if index == 1 {
                            SnapKind::ObjectCenter
                        } else {
                            SnapKind::ObjectEdge
                        },
                    },
                });
            }
        }
    }

    if options.snap_guides {
        for guide in guides {
            if !guide.pos.is_finite() {
                continue;
            }
            match guide.axis {
                Axis::X => candidates_x.push(Candidate {
                    delta: guide.pos - point.x,
                    line: SnapLine {
                        axis: Axis::X,
                        pos: guide.pos,
                        from: point.y - 24.0,
                        to: point.y + 24.0,
                        kind: SnapKind::Guide,
                    },
                }),
                Axis::Y => candidates_y.push(Candidate {
                    delta: guide.pos - point.y,
                    line: SnapLine {
                        axis: Axis::Y,
                        pos: guide.pos,
                        from: point.x - 24.0,
                        to: point.x + 24.0,
                        kind: SnapKind::Guide,
                    },
                }),
            }
        }
    }

    if let Some(grid) = options.grid
        && options.snap_grid
        && grid.size_x > 1e-6
        && grid.size_y > 1e-6
    {
        let nearest_x = snap_to_step(point.x, grid.origin.x, grid.size_x);
        candidates_x.push(Candidate {
            delta: nearest_x - point.x,
            line: SnapLine {
                axis: Axis::X,
                pos: nearest_x,
                from: point.y - 24.0,
                to: point.y + 24.0,
                kind: SnapKind::Grid,
            },
        });
        let nearest_y = snap_to_step(point.y, grid.origin.y, grid.size_y);
        candidates_y.push(Candidate {
            delta: nearest_y - point.y,
            line: SnapLine {
                axis: Axis::Y,
                pos: nearest_y,
                from: point.x - 24.0,
                to: point.x + 24.0,
                kind: SnapKind::Grid,
            },
        });
    }

    let mut result = SnapResult::empty();
    if let Some((delta, lines)) = pick(candidates_x, options.threshold) {
        result.delta.x = delta;
        result.snapped_x = true;
        result.lines.extend(lines);
    }
    if let Some((delta, lines)) = pick(candidates_y, options.threshold) {
        result.delta.y = delta;
        result.snapped_y = true;
        result.lines.extend(lines);
    }
    result
}

fn snap_to_step(value: f64, origin: f64, step: f64) -> f64 {
    origin + ((value - origin) / step).round() * step
}

fn pick(candidates: Vec<Candidate>, threshold: f64) -> Option<(f64, Vec<SnapLine>)> {
    let best = candidates
        .iter()
        .filter(|c| c.delta.abs() <= threshold)
        .min_by(|a, b| a.delta.abs().total_cmp(&b.delta.abs()))?;
    let delta = best.delta;
    // Collect every line that corresponds to the winning correction so the user
    // sees all the alignments that are currently satisfied.
    let lines = candidates
        .into_iter()
        .filter(|c| (c.delta - delta).abs() <= 1e-6)
        .map(|c| c.line)
        .collect();
    Some((delta, lines))
}

/// Chooses the closest alignment among a set of candidate deltas.
pub fn closest_delta(candidates: &[f64], threshold: f64) -> Option<f64> {
    candidates
        .iter()
        .copied()
        .filter(|delta| delta.abs() <= threshold)
        .min_by(|a, b| a.abs().total_cmp(&b.abs()).then(Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_left_edge_to_target_left_edge() {
        let moving = Rect::new(102.0, 100.0, 10.0, 10.0);
        let target = Rect::new(100.0, 100.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], &[], &SnapOptions::default());
        assert!(result.snapped_x);
        assert!(result.snapped_y);
        assert!((result.delta.x + 2.0).abs() < 1e-9);
        assert!(result.delta.y.abs() < 1e-9);
        assert!(
            result
                .lines
                .iter()
                .any(|l| l.axis == Axis::X && (l.pos - 100.0).abs() < 1e-9)
        );
    }

    #[test]
    fn threshold_prevents_snapping() {
        let moving = Rect::new(104.0, 104.0, 10.0, 10.0);
        let target = Rect::new(100.0, 100.0, 50.0, 50.0);
        let options = SnapOptions {
            threshold: 1.0,
            ..SnapOptions::default()
        };
        let result = snap_rect(moving, &[target], &[], &options);
        assert!(!result.is_snapped());
    }

    #[test]
    fn snaps_centers_to_centers() {
        let moving = Rect::new(120.0, 120.0, 10.0, 10.0);
        let target = Rect::new(100.0, 100.0, 50.0, 50.0);
        let options = SnapOptions {
            snap_guides: false,
            ..SnapOptions::default()
        };
        let result = snap_rect(moving, &[target], &[], &options);
        assert!(result.snapped_x);
        // moving.center_x = 125, target.center_x = 125 -> no correction.
        assert!(result.delta.x.abs() < 1e-9);
        assert!(
            result
                .lines
                .iter()
                .any(|l| l.kind == SnapKind::ObjectCenter)
        );
    }

    #[test]
    fn snaps_to_grid() {
        let moving = Rect::new(12.0, 12.0, 10.0, 10.0);
        let options = SnapOptions {
            grid: Some(GridSpec::square(10.0)),
            ..SnapOptions::default()
        };
        let result = snap_rect(moving, &[], &[], &options);
        assert!(result.snapped_x && result.snapped_y);
        assert!((result.delta.x + 2.0).abs() < 1e-9);
        assert!((result.delta.y + 2.0).abs() < 1e-9);
    }

    #[test]
    fn snaps_to_guide() {
        let moving = Rect::new(40.0, 0.0, 10.0, 10.0);
        let guides = [Guide::vertical(42.0)];
        let result = snap_rect(moving, &[], &guides, &SnapOptions::default());
        assert!(result.snapped_x);
        assert!((result.delta.x - 2.0).abs() < 1e-9);
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0].kind, SnapKind::Guide);
        assert_eq!(result.lines[0].axis, Axis::X);
    }

    #[test]
    fn disabled_snapping() {
        let moving = Rect::new(101.0, 101.0, 10.0, 10.0);
        let target = Rect::new(100.0, 100.0, 10.0, 10.0);
        let options = SnapOptions {
            enabled: false,
            ..SnapOptions::default()
        };
        assert_eq!(
            snap_rect(moving, &[target], &[], &options),
            SnapResult::empty()
        );
    }

    #[test]
    fn point_snapping_uses_grid() {
        let options = SnapOptions {
            grid: Some(GridSpec::square(10.0)),
            ..SnapOptions::default()
        };
        let result = snap_point(Point::new(13.0, 12.0), &[], &[], &options);
        assert!(result.snapped_x && result.snapped_y);
        assert!((result.delta.x + 3.0).abs() < 1e-9);
        assert!((result.delta.y + 2.0).abs() < 1e-9);
    }

    #[test]
    fn closest_delta_helper() {
        assert_eq!(closest_delta(&[10.0, -2.0, 3.0], 5.0), Some(-2.0));
        assert_eq!(closest_delta(&[10.0], 5.0), None);
    }
}
