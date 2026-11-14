//! Real polygon boolean operations (M17) on closed vector paths.
//!
//! Paths are flattened to contours (quadratic/cubic curves sampled),
//! combined with `i_overlay` (union/intersect/difference/xor) using the
//! even-odd fill rule, and converted back into [`PathShape`]s. Result
//! shapes keep their outer contour first and holes afterwards, matching
//! the non-zero winding convention used by the SWF exporter.
//!
//! Limitations: curves are flattened (no exact curve booleans), results
//! contain only straight edges, and degenerate outputs are dropped.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use super::{PathCommand, PathShape, Rgba};

/// Boolean operation kinds exposed to the editor and AI agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoolOp {
    Union,
    Intersect,
    /// `a` minus `b`.
    Subtract,
    /// Symmetric difference (XOR / "exclude").
    Exclude,
}

impl BoolOp {
    pub fn label(self) -> &'static str {
        match self {
            Self::Union => "Vereinigung",
            Self::Intersect => "Schnittmenge",
            Self::Subtract => "Subtrahieren",
            Self::Exclude => "Ausschluss (XOR)",
        }
    }
}

type Contour = Vec<[f64; 2]>;
type ShapeContours = Vec<Contour>;
type Shapes = Vec<ShapeContours>;

/// Boolean operation between two objects taking their transforms into
/// account: both path sets are mapped into scene space, combined, and the
/// result mapped back into `a`'s local space so it can replace `a`'s paths.
pub fn boolean_path_objects(
    a_paths: &[PathShape],
    a_transform: &super::Transform,
    b_paths: &[PathShape],
    b_transform: &super::Transform,
    op: BoolOp,
) -> Vec<PathShape> {
    let a_world = transform_paths(a_paths, a_transform);
    let b_world = transform_paths(b_paths, b_transform);
    let mut result = boolean_paths(&a_world, &b_world, op);
    // Map result back into a's local space (exact inverse of the
    // scale→rotate→translate mapping; not representable as one Transform
    // for non-uniform scale + rotation).
    for shape in &mut result {
        for command in &mut shape.commands {
            untransform_command(command, a_transform);
        }
    }
    result
}

/// Inverse of [`transform_command`]: `l = S⁻¹ · R(-rot) · (p - T)`.
fn untransform_command(command: &mut PathCommand, t: &super::Transform) {
    let map = |x: &mut f32, y: &mut f32| {
        let dx = *x - t.x;
        let dy = *y - t.y;
        let rad = (-t.rotation).to_radians();
        let (s, c) = rad.sin_cos();
        let rx = dx * c - dy * s;
        let ry = dx * s + dy * c;
        let sx = if t.scale_x.abs() < 1e-4 {
            if t.scale_x < 0.0 { -1e-4 } else { 1e-4 }
        } else {
            t.scale_x
        };
        let sy = if t.scale_y.abs() < 1e-4 {
            if t.scale_y < 0.0 { -1e-4 } else { 1e-4 }
        } else {
            t.scale_y
        };
        *x = rx / sx;
        *y = ry / sy;
    };
    match command {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => map(x, y),
        PathCommand::CurveTo {
            control_x,
            control_y,
            x,
            y,
        } => {
            map(control_x, control_y);
            map(x, y);
        }
        PathCommand::CubicTo {
            c1_x,
            c1_y,
            c2_x,
            c2_y,
            x,
            y,
        } => {
            map(c1_x, c1_y);
            map(c2_x, c2_y);
            map(x, y);
        }
    }
}

fn transform_command(command: &mut PathCommand, t: &super::Transform) {
    let map = |x: &mut f32, y: &mut f32| {
        let (sx, sy) = (*x * t.scale_x, *y * t.scale_y);
        let rad = t.rotation.to_radians();
        let (s, c) = rad.sin_cos();
        let nx = t.x + sx * c - sy * s;
        let ny = t.y + sx * s + sy * c;
        *x = nx;
        *y = ny;
    };
    match command {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => map(x, y),
        PathCommand::CurveTo {
            control_x,
            control_y,
            x,
            y,
        } => {
            map(control_x, control_y);
            map(x, y);
        }
        PathCommand::CubicTo {
            c1_x,
            c1_y,
            c2_x,
            c2_y,
            x,
            y,
        } => {
            map(c1_x, c1_y);
            map(c2_x, c2_y);
            map(x, y);
        }
    }
}

fn transform_paths(paths: &[PathShape], t: &super::Transform) -> Vec<PathShape> {
    paths
        .iter()
        .map(|p| {
            let mut clone = p.clone();
            for command in &mut clone.commands {
                transform_command(command, t);
            }
            clone
        })
        .collect()
}

/// Applies `op` between the filled regions of `a` and `b` and returns the
/// resulting [`PathShape`]s (one per output shape, holes preserved as
/// additional sub-paths). Returns an empty vec when the result is empty.
pub fn boolean_paths(a: &[PathShape], b: &[PathShape], op: BoolOp) -> Vec<PathShape> {
    let subject: Shapes = a.iter().map(shape_contours).collect();
    let clip: Shapes = b.iter().map(shape_contours).collect();
    if subject.is_empty() || clip.is_empty() {
        return Vec::new();
    }
    let rule = match op {
        BoolOp::Union => OverlayRule::Union,
        BoolOp::Intersect => OverlayRule::Intersect,
        BoolOp::Subtract => OverlayRule::Difference,
        BoolOp::Exclude => OverlayRule::Xor,
    };
    let result = subject.overlay(&clip, rule, FillRule::EvenOdd);
    let fill = a.first().map(|p| p.fill).unwrap_or(Rgba::WHITE);
    result
        .iter()
        .filter_map(|shape| contours_to_path_shape(shape, fill))
        .collect()
}

/// Flattens one [`PathShape`] into contours. Each closed sub-path becomes
/// a contour; open sub-paths are closed implicitly (matching the fill
/// semantics of the renderer).
fn shape_contours(shape: &PathShape) -> ShapeContours {
    let mut contours = Vec::new();
    let mut current: Contour = Vec::new();
    let mut pen = (0.0f64, 0.0f64);
    for command in &shape.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                if current.len() > 2 {
                    contours.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                pen = (x as f64, y as f64);
                current.push([pen.0, pen.1]);
            }
            PathCommand::LineTo { x, y } => {
                pen = (x as f64, y as f64);
                current.push([pen.0, pen.1]);
            }
            PathCommand::CurveTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                let start = pen;
                let (cx, cy) = (control_x as f64, control_y as f64);
                let end = (x as f64, y as f64);
                for step in 1..=12 {
                    let t = step as f64 / 12.0;
                    let mt = 1.0 - t;
                    current.push([
                        mt * mt * start.0 + 2.0 * mt * t * cx + t * t * end.0,
                        mt * mt * start.1 + 2.0 * mt * t * cy + t * t * end.1,
                    ]);
                }
                pen = end;
            }
            PathCommand::CubicTo {
                c1_x,
                c1_y,
                c2_x,
                c2_y,
                x,
                y,
            } => {
                let start = pen;
                let c1 = (c1_x as f64, c1_y as f64);
                let c2 = (c2_x as f64, c2_y as f64);
                let end = (x as f64, y as f64);
                for step in 1..=16 {
                    let t = step as f64 / 16.0;
                    let mt = 1.0 - t;
                    current.push([
                        mt * mt * mt * start.0
                            + 3.0 * mt * mt * t * c1.0
                            + 3.0 * mt * t * t * c2.0
                            + t * t * t * end.0,
                        mt * mt * mt * start.1
                            + 3.0 * mt * mt * t * c1.1
                            + 3.0 * mt * t * t * c2.1
                            + t * t * t * end.1,
                    ]);
                }
                pen = end;
            }
        }
    }
    if current.len() > 2 {
        contours.push(current);
    }
    contours.retain(|c| c.len() >= 3);
    contours
}

/// Converts one result shape (outer contour + holes) back into a
/// [`PathShape`]. Degenerate or duplicate points are removed; contours
/// with fewer than 3 distinct points are dropped.
fn contours_to_path_shape(shape: &ShapeContours, fill: Rgba) -> Option<PathShape> {
    let mut commands = Vec::new();
    for contour in shape {
        let points = dedup_contour(contour);
        if points.len() < 3 {
            continue;
        }
        commands.push(PathCommand::MoveTo {
            x: points[0][0] as f32,
            y: points[0][1] as f32,
        });
        for point in &points[1..] {
            commands.push(PathCommand::LineTo {
                x: point[0] as f32,
                y: point[1] as f32,
            });
        }
        // Close the contour explicitly.
        commands.push(PathCommand::LineTo {
            x: points[0][0] as f32,
            y: points[0][1] as f32,
        });
    }
    if commands.is_empty() {
        None
    } else {
        Some(PathShape {
            fill,
            gradient: None,
            skin: None,
            commands,
        })
    }
}

/// Removes consecutive duplicates (within 1e-4) and a duplicated closing
/// point so contours stay clean.
fn dedup_contour(contour: &Contour) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(contour.len());
    for point in contour {
        if let Some(last) = out.last() {
            let dx = point[0] - last[0];
            let dy = point[1] - last[1];
            if dx * dx + dy * dy < 1e-8 {
                continue;
            }
        }
        out.push(*point);
    }
    // Drop trailing point identical to the first.
    if out.len() > 1 {
        let first = out[0];
        let last = out[out.len() - 1];
        let dx = last[0] - first[0];
        let dy = last[1] - first[1];
        if dx * dx + dy * dy < 1e-8 {
            out.pop();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Rgba;

    fn rect_path(cx: f32, cy: f32, w: f32, h: f32) -> PathShape {
        let (hw, hh) = (w / 2.0, h / 2.0);
        PathShape::new(
            Rgba::WHITE,
            vec![
                PathCommand::MoveTo {
                    x: cx - hw,
                    y: cy - hh,
                },
                PathCommand::LineTo {
                    x: cx + hw,
                    y: cy - hh,
                },
                PathCommand::LineTo {
                    x: cx + hw,
                    y: cy + hh,
                },
                PathCommand::LineTo {
                    x: cx - hw,
                    y: cy + hh,
                },
                PathCommand::LineTo {
                    x: cx - hw,
                    y: cy - hh,
                },
            ],
        )
    }

    fn point_count(shape: &PathShape) -> usize {
        shape
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::LineTo { .. } | PathCommand::MoveTo { .. }))
            .count()
    }

    #[test]
    fn union_of_overlapping_rects_is_single_shape() {
        let a = vec![rect_path(0.0, 0.0, 10.0, 10.0)];
        let b = vec![rect_path(5.0, 5.0, 10.0, 10.0)];
        let result = boolean_paths(&a, &b, BoolOp::Union);
        assert_eq!(result.len(), 1, "union of overlapping rects -> one shape");
        assert!(point_count(&result[0]) >= 8);
    }

    #[test]
    fn subtract_creates_smaller_shape() {
        let a = vec![rect_path(0.0, 0.0, 20.0, 20.0)];
        let b = vec![rect_path(0.0, 0.0, 10.0, 10.0)];
        let result = boolean_paths(&a, &b, BoolOp::Subtract);
        assert!(!result.is_empty());
        assert!(point_count(&result[0]) > 6);
    }

    #[test]
    fn intersect_of_disjoint_rects_is_empty() {
        let a = vec![rect_path(0.0, 0.0, 10.0, 10.0)];
        let b = vec![rect_path(100.0, 0.0, 10.0, 10.0)];
        let result = boolean_paths(&a, &b, BoolOp::Intersect);
        assert!(result.is_empty(), "disjoint intersection must be empty");
    }

    #[test]
    fn xor_of_identical_rects_is_empty() {
        let a = vec![rect_path(0.0, 0.0, 10.0, 10.0)];
        let b = vec![rect_path(0.0, 0.0, 10.0, 10.0)];
        let result = boolean_paths(&a, &b, BoolOp::Exclude);
        assert!(result.is_empty(), "identical shapes XOR -> empty");
    }

    #[test]
    fn subtract_preserves_hole_contours() {
        let mut donut = rect_path(0.0, 0.0, 20.0, 20.0);
        donut.commands.extend(vec![
            PathCommand::MoveTo { x: -3.0, y: -3.0 },
            PathCommand::LineTo { x: -3.0, y: 3.0 },
            PathCommand::LineTo { x: 3.0, y: 3.0 },
            PathCommand::LineTo { x: 3.0, y: -3.0 },
            PathCommand::LineTo { x: -3.0, y: -3.0 },
        ]);
        let blocker = vec![rect_path(8.0, 0.0, 8.0, 8.0)];
        let result = boolean_paths(&[donut], &blocker, BoolOp::Subtract);
        assert!(!result.is_empty());
        let subpaths = result[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::MoveTo { .. }))
            .count();
        assert!(subpaths >= 2, "hole contour must survive, got {subpaths}");
    }

    #[test]
    fn curves_flatten_for_boolean() {
        let mut curved = rect_path(0.0, 0.0, 10.0, 10.0);
        curved.commands.push(PathCommand::CubicTo {
            c1_x: 5.0,
            c1_y: -5.0,
            c2_x: 10.0,
            c2_y: -10.0,
            x: 15.0,
            y: -5.0,
        });
        let other = vec![rect_path(0.0, 0.0, 10.0, 10.0)];
        let result = boolean_paths(&[curved], &other, BoolOp::Intersect);
        let _ = result;
    }
}
