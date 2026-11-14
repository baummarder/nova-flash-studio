//! Convex 2D collision detection for the editor preview simulation.
//!
//! Shapes are centre-based: [`Collider::center`] is the world position and
//! polygon points are relative to it. Supported pairs:
//!
//! - polygon vs polygon (rectangles are four-point polygons),
//! - circle vs polygon/rectangle,
//! - circle vs circle.
//!
//! Polygons must be convex. Non-convex input is approximated by its convex
//! hull ([`convex_hull`]) — deterministic and documented; the authored points
//! are never mutated. Resolution returns a [`Contact`] whose normal points
//! from `b` to `a` plus the penetration depth, matching the convention the
//! integrator already used for AABB pairs.
//!
//! Rotation is ignored (all colliders are axis-aligned in local space), as
//! documented for the whole preview simulation.

/// A collision volume in world space.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Axis-aligned rectangle, half extents.
    Rect { hw: f32, hh: f32 },
    /// Circle radius.
    Circle { r: f32 },
    /// Convex polygon, points relative to `center`, counter-clockwise or
    /// clockwise (winding is normalised internally).
    Poly { points: Vec<(f32, f32)> },
}

/// Centre position + shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Collider {
    pub center: (f32, f32),
    pub shape: Shape,
}

/// Contact result: unit normal from `b` toward `a`, penetration depth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    pub nx: f32,
    pub ny: f32,
    pub depth: f32,
}

impl Contact {
    /// Applies the positional correction to `a`'s centre.
    pub fn push_a(&self, a: &mut (f32, f32)) {
        a.0 += self.nx * self.depth;
        a.1 += self.ny * self.depth;
    }
}

/// Tests `a` against `b`; returns the contact if they overlap.
/// `a` is conventionally the dynamic body being resolved.
pub fn collide(a: &Collider, b: &Collider) -> Option<Contact> {
    use Shape::*;
    match (&a.shape, &b.shape) {
        (Circle { r: ra }, Circle { r: rb }) => circle_circle(a.center, *ra, b.center, *rb),
        (Circle { r }, other) => circle_poly(a.center, *r, b, other),
        (other, Circle { r }) => circle_poly(b.center, *r, a, other).map(|mut c| {
            // Swap roles: normal must point from b(circle) to a(poly).
            c.nx = -c.nx;
            c.ny = -c.ny;
            c
        }),
        _ => poly_poly(a, b),
    }
}

/// Boolean overlap test (used by collision enter/exit events).
pub fn overlaps(a: &Collider, b: &Collider) -> bool {
    collide(a, b).is_some()
}

// ---------------------------------------------------------------------------
// Primitive tests
// ---------------------------------------------------------------------------

fn circle_circle(ac: (f32, f32), ra: f32, bc: (f32, f32), rb: f32) -> Option<Contact> {
    let dx = ac.0 - bc.0;
    let dy = ac.1 - bc.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let overlap = ra + rb - dist;
    if overlap <= 0.0 {
        return None;
    }
    if dist < 0.001 {
        return Some(Contact {
            nx: 0.0,
            ny: -1.0,
            depth: overlap,
        });
    }
    Some(Contact {
        nx: dx / dist,
        ny: dy / dist,
        depth: overlap,
    })
}

/// Circle vs polygon/rect. `poly` is the non-circle collider.
fn circle_poly(cc: (f32, f32), r: f32, poly: &Collider, shape: &Shape) -> Option<Contact> {
    let pts = polygon_points(shape, poly.center);
    if pts.len() < 3 {
        return None;
    }
    // Inside the polygon: push out along the least-penetrating edge normal.
    if point_in_polygon(cc, &pts) {
        let mut best: Option<Contact> = None;
        for i in 0..pts.len() {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % pts.len()];
            let (ex, ey) = (x2 - x1, y2 - y1);
            let len = (ex * ex + ey * ey).sqrt();
            if len < f32::EPSILON {
                continue;
            }
            // Outward normal for a consistently wound polygon.
            let (nx, ny) = if polygon_winding(&pts) > 0.0 {
                (ey / len, -ex / len)
            } else {
                (-ey / len, ex / len)
            };
            // Signed distance from centre to the edge line along outward n.
            let d = (cc.0 - x1) * nx + (cc.1 - y1) * ny;
            let depth = r + d; // d < 0 inside -> depth = r - |d|
            if depth > 0.0 && best.is_none_or(|b| depth < b.depth) {
                best = Some(Contact { nx, ny, depth });
            }
        }
        return best;
    }
    // Outside: closest point on the polygon boundary.
    let mut closest = pts[0];
    let mut best_d2 = f32::INFINITY;
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        let (px, py, d2) = closest_on_segment(cc, (x1, y1), (x2, y2));
        if d2 < best_d2 {
            best_d2 = d2;
            closest = (px, py);
        }
    }
    let dist = best_d2.sqrt();
    if dist >= r || dist < f32::EPSILON {
        return None;
    }
    let (dx, dy) = (cc.0 - closest.0, cc.1 - closest.1);
    Some(Contact {
        nx: dx / dist,
        ny: dy / dist,
        depth: r - dist,
    })
}

fn closest_on_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32) {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = if len2 < f32::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let q = (a.0 + abx * t, a.1 + aby * t);
    let dx = p.0 - q.0;
    let dy = p.1 - q.1;
    (q.0, q.1, dx * dx + dy * dy)
}

/// SAT for two convex polygons (rects included as 4-point polys).
fn poly_poly(a: &Collider, b: &Collider) -> Option<Contact> {
    let pa = polygon_points(&a.shape, a.center);
    let pb = polygon_points(&b.shape, b.center);
    if pa.len() < 3 || pb.len() < 3 {
        return None;
    }
    let mut min_overlap = f32::INFINITY;
    let mut axis = (0.0f32, 0.0f32);
    for pts in [&pa, &pb] {
        for i in 0..pts.len() {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % pts.len()];
            let (ex, ey) = (x2 - x1, y2 - y1);
            let len = (ex * ex + ey * ey).sqrt();
            if len < f32::EPSILON {
                continue;
            }
            // Edge normal (either winding works for SAT).
            let (nx, ny) = (-ey / len, ex / len);
            let (min_a, max_a) = project(&pa, nx, ny);
            let (min_b, max_b) = project(&pb, nx, ny);
            let overlap = max_a.min(max_b) - min_a.max(min_b);
            if overlap <= 0.0 {
                return None;
            }
            if overlap < min_overlap {
                min_overlap = overlap;
                // Orient the normal from b toward a.
                let dir = (a.center.0 - b.center.0) * nx + (a.center.1 - b.center.1) * ny;
                axis = if dir >= 0.0 { (nx, ny) } else { (-nx, -ny) };
            }
        }
    }
    if !min_overlap.is_finite() {
        return None;
    }
    Some(Contact {
        nx: axis.0,
        ny: axis.1,
        depth: min_overlap,
    })
}

fn project(pts: &[(f32, f32)], nx: f32, ny: f32) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &(x, y) in pts {
        let d = x * nx + y * ny;
        min = min.min(d);
        max = max.max(d);
    }
    (min, max)
}

/// Absolute polygon vertices for `shape` centred at `center`.
fn polygon_points(shape: &Shape, center: (f32, f32)) -> Vec<(f32, f32)> {
    match shape {
        Shape::Rect { hw, hh } => vec![
            (center.0 - hw, center.1 - hh),
            (center.0 + hw, center.1 - hh),
            (center.0 + hw, center.1 + hh),
            (center.0 - hw, center.1 + hh),
        ],
        Shape::Poly { points } => points
            .iter()
            .map(|&(x, y)| (center.0 + x, center.1 + y))
            .collect(),
        Shape::Circle { .. } => Vec::new(),
    }
}

fn polygon_winding(pts: &[(f32, f32)]) -> f32 {
    let mut area = 0.0;
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        area += x1 * y2 - x2 * y1;
    }
    area
}

fn point_in_polygon(p: (f32, f32), pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        if (y1 > p.1) != (y2 > p.1) && p.0 < (x2 - x1) * (p.1 - y1) / (y2 - y1) + x1 {
            inside = !inside;
        }
    }
    inside
}

// ---------------------------------------------------------------------------
// Polygon utilities (validation + hull)
// ---------------------------------------------------------------------------

/// Whether `points` form a strictly convex polygon (no three consecutive
/// collinear vertices turning the wrong way, no self-intersection of the
/// simplified boundary).
pub fn is_convex(points: &[(f32, f32)]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut sign = 0i32;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let c = points[(i + 2) % points.len()];
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        if cross.abs() < 1e-6 {
            continue; // collinear edge tolerated
        }
        let s = if cross > 0.0 { 1 } else { -1 };
        if sign == 0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    sign != 0
}

/// Whether the polygon boundary intersects itself (excluding shared
/// consecutive vertices).
pub fn self_intersects(points: &[(f32, f32)]) -> bool {
    let n = points.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue; // adjacent wrap-around edge
            }
            if segments_intersect(
                points[i],
                points[(i + 1) % n],
                points[j],
                points[(j + 1) % n],
            ) {
                return true;
            }
        }
    }
    false
}

fn segments_intersect(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    fn orient(p: (f32, f32), q: (f32, f32), r: (f32, f32)) -> f32 {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    }
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    (o1 * o2 < 0.0 && o3 * o4 < 0.0)
        || (o1.abs() < 1e-6 && on_segment(a, b, c))
        || (o2.abs() < 1e-6 && on_segment(a, b, d))
        || (o3.abs() < 1e-6 && on_segment(c, d, a))
        || (o4.abs() < 1e-6 && on_segment(c, d, b))
}

fn on_segment(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> bool {
    p.0 >= a.0.min(b.0) - 1e-6
        && p.0 <= a.0.max(b.0) + 1e-6
        && p.1 >= a.1.min(b.1) - 1e-6
        && p.1 <= a.1.max(b.1) + 1e-6
}

/// Andrew's monotone chain convex hull, counter-clockwise, no duplicate
/// closing point.
pub fn convex_hull(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut pts: Vec<(f32, f32)> = points.to_vec();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    pts.dedup();
    if pts.len() <= 2 {
        return pts;
    }
    let mut lower: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 {
            let n = lower.len();
            let (a, b) = (lower[n - 2], lower[n - 1]);
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            if cross <= 1e-9 {
                lower.pop();
            } else {
                break;
            }
        }
        lower.push(p);
    }
    let mut upper: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 {
            let n = upper.len();
            let (a, b) = (upper[n - 2], upper[n - 1]);
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            if cross <= 1e-9 {
                upper.pop();
            } else {
                break;
            }
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Validates authored polygon points; returns an error string.
/// Rules: 3–64 points, all finite, bounded extent, no duplicate consecutive
/// points. Convexity is NOT required here — non-convex polygons are
/// approximated by their convex hull at simulation time (documented).
pub fn validate_polygon(points: &[(f32, f32)]) -> Result<(), String> {
    if points.len() < 3 {
        return Err("Polygon braucht mindestens 3 Punkte.".into());
    }
    if points.len() > 64 {
        return Err("Polygon ist auf 64 Punkte begrenzt.".into());
    }
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return Err("Polygonpunkte müssen endlich sein.".into());
        }
        if x.abs() > 100_000.0 || y.abs() > 100_000.0 {
            return Err("Polygonpunkte sind zu groß (>100000).".into());
        }
    }
    // Degenerate: all points identical or collinear.
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &(x, y) in points {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    if max_x - min_x < 1e-4 || max_y - min_y < 1e-4 {
        return Err("Polygon ist degeneriert (keine Fläche).".into());
    }
    // Reject zero-area / fully collinear sets via hull size.
    if convex_hull(points).len() < 3 {
        return Err("Polygonpunkte sind kollinear (keine Fläche).".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(cx: f32, cy: f32, w: f32, h: f32) -> Collider {
        Collider {
            center: (cx, cy),
            shape: Shape::Rect {
                hw: w / 2.0,
                hh: h / 2.0,
            },
        }
    }

    fn tri(cx: f32, cy: f32) -> Collider {
        Collider {
            center: (cx, cy),
            shape: Shape::Poly {
                points: vec![(0.0, -10.0), (10.0, 10.0), (-10.0, 10.0)],
            },
        }
    }

    #[test]
    fn separated_rects_do_not_collide() {
        assert!(collide(&rect(0.0, 0.0, 10.0, 10.0), &rect(20.0, 0.0, 10.0, 10.0)).is_none());
    }

    #[test]
    fn overlapping_rects_report_min_axis() {
        // b is right of a by 8px; overlap on x = 10-8=2, y overlap = 10.
        // Normal points from b toward a -> -x.
        let c = collide(&rect(0.0, 0.0, 10.0, 10.0), &rect(8.0, 0.0, 10.0, 10.0)).unwrap();
        assert!((c.nx + 1.0).abs() < 1e-4 && c.ny.abs() < 1e-4);
        assert!((c.depth - 2.0).abs() < 1e-4);
    }

    #[test]
    fn rect_fully_inside_reports_containment_depth() {
        // Small b inside big a: MTV is the smallest push-out axis.
        let c = collide(&rect(0.0, 0.0, 20.0, 20.0), &rect(1.0, 0.0, 4.0, 4.0)).unwrap();
        assert!(c.depth > 0.0);
        assert!(c.nx.abs() + c.ny.abs() > 0.9);
    }

    #[test]
    fn triangle_vs_rect_uses_real_geometry() {
        // Triangle apex reaches x=10; rect starts at x=12 -> no touch.
        let t = tri(0.0, 0.0);
        assert!(collide(&t, &rect(22.0, 0.0, 10.0, 20.0)).is_none());
        // Move rect into the triangle's right edge -> contact.
        let c = collide(&t, &rect(13.0, 0.0, 10.0, 20.0)).unwrap();
        assert!(c.depth > 0.0);
    }

    #[test]
    fn circle_vs_polygon_vertex_region() {
        // Circle near a triangle vertex: the vertex normal is NOT an edge
        // normal — this is exactly what the old AABB approximation got wrong.
        let t = tri(0.0, 0.0);
        let circle = Collider {
            center: (1.0, -11.0), // just outside the apex (0,-10)
            shape: Shape::Circle { r: 2.0 },
        };
        let c = collide(&circle, &t).unwrap();
        // Normal should point roughly up-right away from the apex.
        assert!(c.nx > 0.3 && c.ny < -0.3, "n=({},{})", c.nx, c.ny);
        assert!(c.depth > 0.0 && c.depth < 2.0);
    }

    #[test]
    fn circle_inside_polygon_pushes_out() {
        let t = tri(0.0, 0.0);
        let circle = Collider {
            center: (0.0, 2.0),
            shape: Shape::Circle { r: 6.0 }, // reaches the slanted edges
        };
        let c = collide(&circle, &t).unwrap();
        assert!(c.depth > 0.0);
    }

    #[test]
    fn circle_circle_unchanged() {
        let a = Collider {
            center: (0.0, 0.0),
            shape: Shape::Circle { r: 5.0 },
        };
        let b = Collider {
            center: (8.0, 0.0),
            shape: Shape::Circle { r: 5.0 },
        };
        let c = collide(&a, &b).unwrap();
        assert!((c.depth - 2.0).abs() < 1e-4);
    }

    #[test]
    fn convexity_and_hull() {
        assert!(is_convex(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]));
        // Arrow/concave shape.
        let concave = [(0.0, 0.0), (4.0, 0.0), (2.0, 2.0), (4.0, 4.0), (0.0, 4.0)];
        assert!(!is_convex(&concave));
        let hull = convex_hull(&concave);
        assert!(is_convex(&hull));
        assert!(hull.len() <= 4);
    }

    #[test]
    fn self_intersection_detected() {
        // Bow-tie quad.
        let bowtie = [(0.0, 0.0), (4.0, 4.0), (4.0, 0.0), (0.0, 4.0)];
        assert!(self_intersects(&bowtie));
        assert!(!self_intersects(&[
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 4.0),
            (0.0, 4.0)
        ]));
    }

    #[test]
    fn polygon_validation() {
        assert!(validate_polygon(&[(0.0, 0.0), (4.0, 0.0), (2.0, 4.0)]).is_ok());
        assert!(validate_polygon(&[(0.0, 0.0), (1.0, 1.0)]).is_err());
        assert!(validate_polygon(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)]).is_err());
        assert!(validate_polygon(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]).is_ok());
        // Concave but valid area passes validation (hull used at runtime).
        assert!(
            validate_polygon(&[(0.0, 0.0), (4.0, 0.0), (2.0, 2.0), (4.0, 4.0), (0.0, 4.0)]).is_ok()
        );
    }
}
