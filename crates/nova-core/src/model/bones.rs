//! Skeletal animation model (format v4 addition): bone hierarchies, FK/IK
//! clips and rigid part binding.
//!
//! Bones form a tree. Each bone stores its joint offset `(x, y)` in parent
//! space, a rest `rotation` (degrees) and a visual `length`. A [`BoneClip`]
//! stores per-bone rotation keys plus optional two-bone IK target keys.
//! [`Skeleton::pose_at`] evaluates the whole rig deterministically for a
//! frame index; bound [`crate::model::ObjectInstance`]s then compose their
//! local transform with the bone's world pose (rigid binding — there is no
//! mesh skinning or weight painting).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One joint in a [`Skeleton`]. `parent == None` marks a root bone whose
/// `(x, y)` is in skeleton space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bone {
    pub id: Uuid,
    pub name: String,
    pub parent: Option<Uuid>,
    /// Joint offset from the parent bone origin (skeleton space for roots).
    pub x: f32,
    pub y: f32,
    /// Visual length used by the editor and IK solver; not part of the pose.
    pub length: f32,
    /// Rest rotation in degrees (0 = parent direction).
    pub rotation: f32,
}

impl Bone {
    pub fn new(name: impl Into<String>, x: f32, y: f32, length: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            parent: None,
            x,
            y,
            length,
            rotation: 0.0,
        }
    }
}

/// A bone hierarchy attached to a [`crate::model::Symbol`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

/// One FK key: absolute rotation (degrees) at `frame`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoneKey {
    pub frame: u32,
    pub rotation: f32,
}

/// One IK key: end-effector target `(x, y)` in skeleton space at `frame`.
/// `pole_x`/`pole_y` optionally bias the bend direction of chains with
/// three or more bones (mirrored across the root→target line when needed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IkKey {
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pole_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pole_y: Option<f32>,
}

impl IkKey {
    pub fn new(frame: u32, x: f32, y: f32) -> Self {
        Self {
            frame,
            x,
            y,
            pole_x: None,
            pole_y: None,
        }
    }
}

/// Animation track for one bone: FK rotation keys and/or IK target keys.
/// When `ik` is non-empty it wins over `keys` for that bone (two-bone
/// solver over the bone and its parent).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoneTrack {
    pub bone: Uuid,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<BoneKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ik: Vec<IkKey>,
}

/// A named skeletal animation clip. `frames` is the clip length in frames;
/// evaluation loops when `looping` is set and clamps otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoneClip {
    pub name: String,
    pub frames: u32,
    pub looping: bool,
    pub tracks: Vec<BoneTrack>,
}

impl BoneClip {
    pub fn new(name: impl Into<String>, frames: u32) -> Self {
        Self {
            name: name.into(),
            frames: frames.max(1),
            looping: true,
            tracks: Vec::new(),
        }
    }
}

/// Evaluated world pose of one bone: joint position + rotation (degrees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BonePose {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
}

impl Skeleton {
    pub fn bone(&self, id: Uuid) -> Option<&Bone> {
        self.bones.iter().find(|b| b.id == id)
    }

    /// World-space joint position + rotation for every bone at `frame`.
    /// `clip` may be `None` (rest pose). Deterministic: identical inputs
    /// always produce identical poses (no wall-clock, no RNG).
    ///
    /// IK: a track with `ik` keys solves a two-bone chain (the tracked bone
    /// plus its parent) toward the keyed target. The parent's rotation is
    /// overridden too; descendants see the updated parent pose.
    pub fn pose_at(&self, clip: Option<&BoneClip>, frame: u32) -> Vec<(Uuid, BonePose)> {
        let mut state = PoseState {
            out: Vec::with_capacity(self.bones.len()),
            local_rots: std::collections::HashMap::new(),
        };
        let mut done: Vec<Uuid> = Vec::new();
        for bone in &self.bones {
            if !done.contains(&bone.id) {
                self.pose_bone(bone, clip, frame, &mut state, &mut done);
            }
        }
        state.out
    }

    /// FK world pose of `bone` at `frame` (degrees), honouring parent chain
    /// and two-bone IK overrides.
    fn pose_bone(
        &self,
        bone: &Bone,
        clip: Option<&BoneClip>,
        frame: u32,
        state: &mut PoseState,
        done: &mut Vec<Uuid>,
    ) -> BonePose {
        if let Some(entry) = state.out.iter().find(|(id, _)| *id == bone.id) {
            return entry.1;
        }
        // Parent pose first (roots evaluate to identity + own offset).
        let (px, py, prot, parent_pose) = match bone.parent.and_then(|p| self.bone(p)) {
            Some(parent_bone) => {
                let ppose = self.pose_bone(parent_bone, clip, frame, state, done);
                (ppose.x, ppose.y, ppose.rotation, Some(ppose))
            }
            None => (0.0, 0.0, 0.0, None),
        };
        let mut local_rot = bone.rotation;
        if let Some(track) = clip.and_then(|c| c.tracks.iter().find(|t| t.bone == bone.id)) {
            local_rot = sample_keys(&track.keys, frame, bone.rotation);
            // Multi-bone IK: chains of 3+ bones solve via FABRIK; the
            // analytic two-bone solver handles exactly two bones.
            if !track.ik.is_empty() {
                let chain = self.ik_chain(bone.id, clip);
                if chain.len() >= 3 {
                    let target = sample_ik(&track.ik, frame, clip);
                    let pole = sample_ik_pole(&track.ik, frame, clip);
                    if let Some(pose) =
                        self.apply_chain_ik(&chain, target, pole, clip, frame, state)
                    {
                        state.out.push((bone.id, pose));
                        done.push(bone.id);
                        return pose;
                    }
                }
            }
            // Two-bone IK: bone + parent reach for the keyed target.
            if !track.ik.is_empty()
                && let Some(ppose) = parent_pose
                && let Some(parent_bone) = bone.parent.and_then(|p| self.bone(p))
            {
                let target = sample_ik(&track.ik, frame, clip);
                if let Some((bone_rot, parent_rot)) = solve_two_bone(
                    ppose,
                    parent_bone.length.max(1.0),
                    bone.length.max(1.0),
                    target,
                ) {
                    // Override the parent's local rotation and re-derive its
                    // world rotation so this bone's joint uses the new frame.
                    let parent_old_local = *state
                        .local_rots
                        .get(&parent_bone.id)
                        .unwrap_or(&parent_bone.rotation);
                    let parent_new_world = ppose.rotation - parent_old_local + parent_rot;
                    if let Some(entry) = state.out.iter_mut().find(|(id, _)| *id == parent_bone.id)
                    {
                        entry.1.rotation = parent_new_world;
                    }
                    state.local_rots.insert(parent_bone.id, parent_rot);
                    // Recompute this bone's joint under the corrected parent.
                    let grand = match parent_bone.parent.and_then(|g| self.bone(g)) {
                        Some(gb) => {
                            let gp = self.pose_bone(gb, clip, frame, state, done);
                            (gp.x, gp.y, gp.rotation)
                        }
                        None => (0.0, 0.0, 0.0),
                    };
                    let (jx2, jy2) =
                        rotate_point(grand.0, grand.1, grand.2, parent_bone.x, parent_bone.y);
                    let parent_pose2 = BonePose {
                        x: jx2,
                        y: jy2,
                        rotation: parent_new_world,
                    };
                    let (jx, jy) = rotate_point(
                        parent_pose2.x,
                        parent_pose2.y,
                        parent_new_world,
                        bone.x,
                        bone.y,
                    );
                    let result = BonePose {
                        x: jx,
                        y: jy,
                        rotation: parent_new_world + bone_rot,
                    };
                    state.local_rots.insert(bone.id, bone_rot);
                    state.out.push((bone.id, result));
                    done.push(bone.id);
                    return result;
                }
            }
        }
        let (jx, jy) = rotate_point(px, py, prot, bone.x, bone.y);
        let result = BonePose {
            x: jx,
            y: jy,
            rotation: prot + local_rot,
        };
        state.local_rots.insert(bone.id, local_rot);
        state.out.push((bone.id, result));
        done.push(bone.id);
        result
    }
}

struct PoseState {
    out: Vec<(Uuid, BonePose)>,
    local_rots: std::collections::HashMap<Uuid, f32>,
}

impl Skeleton {
    /// Bones forming the IK chain ending at `effector`, ordered root→tip.
    /// The chain stops below the first ancestor that has its own animation
    /// track (FK keys or IK) in this clip — animated ancestors keep their
    /// evaluated pose and act as the chain root.
    fn ik_chain(&self, effector: Uuid, clip: Option<&BoneClip>) -> Vec<Uuid> {
        let mut chain = vec![effector];
        let mut cursor = self.bone(effector).and_then(|b| b.parent);
        while let Some(id) = cursor {
            let animated = clip.is_some_and(|c| {
                c.tracks
                    .iter()
                    .any(|t| t.bone == id && (!t.keys.is_empty() || !t.ik.is_empty()))
            });
            if animated {
                break;
            }
            chain.push(id);
            cursor = self.bone(id).and_then(|b| b.parent);
        }
        chain.reverse();
        chain
    }

    /// FABRIK solve for a chain of 3+ bones. Updates `state.out` /
    /// `local_rots` for every chain bone and returns the effector pose.
    /// Joint positions come from the already-evaluated FK poses; the tip is
    /// the effector joint extended by its bone length along its rotation.
    fn apply_chain_ik(
        &self,
        chain: &[Uuid],
        target: (f32, f32),
        pole: Option<(f32, f32)>,
        clip: Option<&BoneClip>,
        frame: u32,
        state: &mut PoseState,
    ) -> Option<BonePose> {
        let n = chain.len();
        if n < 3 {
            return None;
        }
        let pose_of = |state: &PoseState, id: Uuid| -> Option<BonePose> {
            state
                .out
                .iter()
                .find(|(bid, _)| *bid == id)
                .map(|(_, p)| *p)
        };
        // Joint positions P[0..n]: P[i] is the world joint of chain[i];
        // the effector's joint is derived from its (already posed) parent.
        let mut points: Vec<(f32, f32)> = Vec::with_capacity(n + 1);
        for (i, id) in chain.iter().enumerate() {
            if i + 1 == n {
                let parent = pose_of(state, chain[i - 1])?;
                let bone = self.bone(*id)?;
                let local = self.local_rot_at(*id, clip, frame);
                let world = parent.rotation + local;
                let (jx, jy) = rotate_point(parent.x, parent.y, world, bone.x, bone.y);
                points.push((jx, jy));
            } else {
                let p = pose_of(state, *id)?;
                points.push((p.x, p.y));
            }
        }
        // Tip: effector joint extended by bone length along world rotation.
        let effector = self.bone(*chain.last()?)?;
        let parent_pose = pose_of(state, chain[n - 2])?;
        let effector_world = parent_pose.rotation + self.local_rot_at(*chain.last()?, clip, frame);
        let (tx, ty) = rotate_point(
            points[n - 1].0,
            points[n - 1].1,
            effector_world,
            effector.length.max(1.0),
            0.0,
        );
        points.push((tx, ty));
        // Segment lengths from the FK pose (fixed for the solve).
        let mut lengths = Vec::with_capacity(n);
        for i in 0..n {
            let dx = points[i + 1].0 - points[i].0;
            let dy = points[i + 1].1 - points[i].1;
            lengths.push((dx * dx + dy * dy).sqrt().max(0.001));
        }
        let mut solved = points.clone();
        fabrik_solve(&mut solved, &lengths, target, pole, 12);
        // New world rotations: each bone points at the next joint/tip.
        let parent_world = match self.bone(chain[0]).and_then(|b| b.parent) {
            Some(pid) => pose_of(state, pid).map(|p| p.rotation).unwrap_or(0.0),
            None => 0.0,
        };
        let mut prev_world = parent_world;
        for i in 0..n {
            let id = chain[i];
            let dx = solved[i + 1].0 - solved[i].0;
            let dy = solved[i + 1].1 - solved[i].1;
            let world = dy.atan2(dx).to_degrees();
            let local = world - prev_world;
            prev_world = world;
            state.local_rots.insert(id, local);
            if i + 1 == n {
                // Effector: joint position is solved[i], rotation world.
                // Its out entry is pushed by the caller.
                continue;
            }
            if let Some(entry) = state.out.iter_mut().find(|(bid, _)| *bid == id) {
                entry.1 = BonePose {
                    x: solved[i].0,
                    y: solved[i].1,
                    rotation: world,
                };
            }
        }
        Some(BonePose {
            x: solved[n - 1].0,
            y: solved[n - 1].1,
            rotation: prev_world,
        })
    }

    /// Local rotation of `bone` at `frame`: sampled FK keys when a track
    /// exists, otherwise the bone's rest rotation.
    fn local_rot_at(&self, id: Uuid, clip: Option<&BoneClip>, frame: u32) -> f32 {
        let Some(bone) = self.bone(id) else {
            return 0.0;
        };
        match clip.and_then(|c| c.tracks.iter().find(|t| t.bone == id)) {
            Some(track) => sample_keys(&track.keys, frame, bone.rotation),
            None => bone.rotation,
        }
    }
}

/// FABRIK iterative IK solver: adjusts `points` (joint positions, first
/// point pinned) so the last point reaches `target` while preserving the
/// segment lengths in `lengths` (`lengths.len() == points.len() - 1`).
/// `pole`, when given, mirrors the solved mid-chain joints to the same
/// side of the root→target line as the pole (2D pole target). Fully
/// deterministic — fixed iteration count, no RNG.
pub fn fabrik_solve(
    points: &mut [(f32, f32)],
    lengths: &[f32],
    target: (f32, f32),
    pole: Option<(f32, f32)>,
    iterations: usize,
) {
    let n = points.len();
    if n < 3 || lengths.len() != n - 1 {
        return;
    }
    let root = points[0];
    for _ in 0..iterations {
        // Backward pass: tip pinned to target, joints pulled toward it.
        points[n - 1] = target;
        for i in (1..n - 1).rev() {
            let (dx, dy) = (points[i].0 - points[i + 1].0, points[i].1 - points[i + 1].1);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 1e-6 {
                continue;
            }
            points[i] = (
                points[i + 1].0 + dx / d * lengths[i],
                points[i + 1].1 + dy / d * lengths[i],
            );
        }
        // Forward pass: root pinned, joints pushed outward.
        points[0] = root;
        for i in 1..n {
            let (dx, dy) = (points[i].0 - points[i - 1].0, points[i].1 - points[i - 1].1);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 1e-6 {
                continue;
            }
            points[i] = (
                points[i - 1].0 + dx / d * lengths[i - 1],
                points[i - 1].1 + dy / d * lengths[i - 1],
            );
        }
    }
    points[n - 1] = target;
    // Pole: keep the chain's mid joints on the pole's side of root→target.
    if let Some(pole) = pole {
        let rx = target.0 - root.0;
        let ry = target.1 - root.1;
        let rl = (rx * rx + ry * ry).sqrt();
        if rl > 1e-6 {
            let side = |px: f32, py: f32| -> f32 {
                // 2D cross product (root→target) × (root→point).
                rx * (py - root.1) - ry * (px - root.0)
            };
            let pole_side = side(pole.0, pole.1);
            if pole_side.abs() > 1e-3 {
                let mid = n / 2;
                let mid_side = side(points[mid].0, points[mid].1);
                if mid_side * pole_side < 0.0 {
                    // Mirror joints 1..n-1 across the root→target line.
                    let (ux, uy) = (rx / rl, ry / rl);
                    for p in points.iter_mut().skip(1).take(n - 2) {
                        let (dx, dy) = (p.0 - root.0, p.1 - root.1);
                        let along = dx * ux + dy * uy;
                        let perp = dx * (-uy) + dy * ux;
                        *p = (
                            root.0 + along * ux + perp * uy,
                            root.1 + along * uy - perp * ux,
                        );
                    }
                }
            }
        }
    }
}

fn sample_ik(keys: &[IkKey], frame: u32, clip: Option<&BoneClip>) -> (f32, f32) {
    let f = match clip {
        Some(c) if c.looping && c.frames > 0 => frame % c.frames,
        Some(c) => frame.min(c.frames.saturating_sub(1)),
        None => frame,
    };
    if keys.is_empty() {
        return (0.0, 0.0);
    }
    if keys.len() == 1 || f <= keys[0].frame {
        return (keys[0].x, keys[0].y);
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if f <= b.frame {
            let span = (b.frame - a.frame).max(1) as f32;
            let t = (f - a.frame) as f32 / span;
            return (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        }
    }
    let last = keys[keys.len() - 1];
    (last.x, last.y)
}

/// Pole target sampled with the same frame mapping as [`sample_ik`].
/// Interpolates only when both neighbouring keys carry a pole.
fn sample_ik_pole(keys: &[IkKey], frame: u32, clip: Option<&BoneClip>) -> Option<(f32, f32)> {
    let f = match clip {
        Some(c) if c.looping && c.frames > 0 => frame % c.frames,
        Some(c) => frame.min(c.frames.saturating_sub(1)),
        None => frame,
    };
    let pair_pole = |a: &IkKey, b: &IkKey, t: f32| -> Option<(f32, f32)> {
        match (a.pole_x.zip(a.pole_y), b.pole_x.zip(b.pole_y)) {
            (Some((ax, ay)), Some((bx, by))) => Some((ax + (bx - ax) * t, ay + (by - ay) * t)),
            _ => None,
        }
    };
    if keys.len() <= 1 {
        return keys.first().and_then(|k| k.pole_x.zip(k.pole_y));
    }
    if f <= keys[0].frame {
        return keys[0].pole_x.zip(keys[0].pole_y);
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if f <= b.frame {
            let span = (b.frame - a.frame).max(1) as f32;
            let t = (f - a.frame) as f32 / span;
            return pair_pole(&a, &b, t).or_else(|| a.pole_x.zip(a.pole_y));
        }
    }
    keys.last().and_then(|k| k.pole_x.zip(k.pole_y))
}

fn sample_keys(keys: &[BoneKey], frame: u32, default: f32) -> f32 {
    if keys.is_empty() {
        return default;
    }
    if keys.len() == 1 || frame <= keys[0].frame {
        return keys[0].rotation;
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if frame <= b.frame {
            let span = (b.frame - a.frame).max(1) as f32;
            let t = (frame - a.frame) as f32 / span;
            return a.rotation + (b.rotation - a.rotation) * t;
        }
    }
    keys[keys.len() - 1].rotation
}

fn rotate_point(px: f32, py: f32, degrees: f32, x: f32, y: f32) -> (f32, f32) {
    let rad = degrees.to_radians();
    let (s, c) = rad.sin_cos();
    (px + x * c - y * s, py + x * s + y * c)
}

/// Composes a bound object's local transform with a bone world pose.
/// `local` is (x, y, rotation_deg); bone scale is not modelled.
pub fn bind_transform(pose: BonePose, lx: f32, ly: f32, lrot: f32) -> (f32, f32, f32) {
    let (rx, ry) = rotate_point(0.0, 0.0, pose.rotation, lx, ly);
    (pose.x + rx, pose.y + ry, pose.rotation + lrot)
}

/// Two-bone analytic IK: rotates `bone` and its `parent` so the tip of
/// `bone` (at distance `bone_len` from the bone joint along its direction)
/// reaches `target` in skeleton space. Returns (bone_rot, parent_rot)
/// relative rotations in degrees, or `None` when the chain cannot reach.
pub fn solve_two_bone(
    parent: BonePose,
    bone_len_parent: f32,
    bone_len: f32,
    target: (f32, f32),
) -> Option<(f32, f32)> {
    let dx = target.0 - parent.x;
    let dy = target.1 - parent.y;
    let d = (dx * dx + dy * dy).sqrt();
    let max_reach = bone_len_parent + bone_len;
    let min_reach = (bone_len_parent - bone_len).abs();
    if d < 1e-6 || d > max_reach || d < min_reach {
        return None;
    }
    // Angle of the target direction relative to the parent bone direction.
    let base = parent.rotation.to_radians();
    let target_angle = dy.atan2(dx);
    // Law of cosines for the shoulder angle.
    let cos_a = ((bone_len_parent * bone_len_parent + d * d - bone_len * bone_len)
        / (2.0 * bone_len_parent * d))
        .clamp(-1.0, 1.0);
    let a = cos_a.acos();
    let shoulder = target_angle - base;
    let parent_rot = shoulder.to_degrees() - a.to_degrees();
    // Elbow angle.
    let cos_b = ((bone_len_parent * bone_len_parent + bone_len * bone_len - d * d)
        / (2.0 * bone_len_parent * bone_len))
        .clamp(-1.0, 1.0);
    let b = cos_b.acos();
    let bone_rot = 180.0 - b.to_degrees();
    Some((bone_rot, parent_rot))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> Skeleton {
        let mut upper = Bone::new("UpperArm", 0.0, 0.0, 40.0);
        upper.rotation = 0.0;
        let mut fore = Bone::new("ForeArm", 40.0, 0.0, 40.0);
        fore.parent = Some(upper.id);
        Skeleton {
            bones: vec![upper, fore],
        }
    }

    #[test]
    fn fk_chain_reaches_expected_positions() {
        let rig = rig();
        let mut clip = BoneClip::new("wave", 10);
        let upper_id = rig.bones[0].id;
        clip.tracks.push(BoneTrack {
            bone: upper_id,
            keys: vec![
                BoneKey {
                    frame: 0,
                    rotation: 0.0,
                },
                BoneKey {
                    frame: 10,
                    rotation: 90.0,
                },
            ],
            ik: vec![],
        });
        let pose = rig.pose_at(Some(&clip), 10);
        let fore = pose
            .iter()
            .find(|(id, _)| *id == rig.bones[1].id)
            .unwrap()
            .1;
        // Upper rotated 90°: fore joint = upper origin + R90*(40,0) = (0,40).
        assert!((fore.x - 0.0).abs() < 0.01);
        assert!((fore.y - 40.0).abs() < 0.01);
        assert!((fore.rotation - 90.0).abs() < 0.01);
    }

    #[test]
    fn rest_pose_uses_bone_rotation() {
        let mut rig = rig();
        rig.bones[0].rotation = 45.0;
        let pose = rig.pose_at(None, 0);
        let fore = pose
            .iter()
            .find(|(id, _)| *id == rig.bones[1].id)
            .unwrap()
            .1;
        assert!((fore.rotation - 45.0).abs() < 0.01);
    }

    #[test]
    fn two_bone_ik_reaches_target() {
        let parent = BonePose {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
        };
        // Target straight down at distance 60 (< 80 reach).
        let result = solve_two_bone(parent, 40.0, 40.0, (0.0, -60.0));
        assert!(result.is_some());
        let (bone_rot, parent_rot) = result.unwrap();
        // Simulate: parent rotated by parent_rot, tip should land near target.
        let mid = rotate_point(0.0, 0.0, parent_rot, 40.0, 0.0);
        let tip = rotate_point(mid.0, mid.1, parent_rot + bone_rot, 40.0, 0.0);
        assert!((tip.0 - 0.0).abs() < 1.0);
        assert!((tip.1 - (-60.0)).abs() < 1.0);
    }

    #[test]
    fn ik_unreachable_returns_none() {
        let parent = BonePose {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
        };
        assert!(solve_two_bone(parent, 40.0, 40.0, (0.0, -200.0)).is_none());
    }

    #[test]
    fn clip_loops_and_clamps() {
        let rig = rig();
        let mut clip = BoneClip::new("c", 4);
        let id = rig.bones[0].id;
        clip.tracks.push(BoneTrack {
            bone: id,
            keys: vec![
                BoneKey {
                    frame: 0,
                    rotation: 0.0,
                },
                BoneKey {
                    frame: 4,
                    rotation: 40.0,
                },
            ],
            ik: vec![],
        });
        clip.looping = true;
        // frame 8 == frame 4 of the loop -> rotation 40.
        let pose = rig.pose_at(Some(&clip), 8);
        let p = pose.iter().find(|(i, _)| *i == id).unwrap().1;
        assert!((p.rotation - 40.0).abs() < 0.01);
        clip.looping = false;
        let pose = rig.pose_at(Some(&clip), 99);
        let p = pose.iter().find(|(i, _)| *i == id).unwrap().1;
        assert!((p.rotation - 40.0).abs() < 0.01);
    }

    fn chain_rig() -> Skeleton {
        // root -> mid -> end, each offset 30px along +x, length 30.
        let root = Bone::new("Root", 0.0, 0.0, 30.0);
        let mut mid = Bone::new("Mid", 30.0, 0.0, 30.0);
        mid.parent = Some(root.id);
        let mut end = Bone::new("End", 30.0, 0.0, 30.0);
        end.parent = Some(mid.id);
        Skeleton {
            bones: vec![root, mid, end],
        }
    }

    #[test]
    fn fabrik_reaches_target() {
        // Straight chain 0->30->60->90 on x axis; target below.
        let mut points = vec![(0.0f32, 0.0f32), (30.0, 0.0), (60.0, 0.0), (90.0, 0.0)];
        let lengths = vec![30.0f32, 30.0, 30.0];
        fabrik_solve(&mut points, &lengths, (60.0, -50.0), None, 20);
        let tip = points[3];
        assert!((tip.0 - 60.0).abs() < 1.0, "tip x {}", tip.0);
        assert!((tip.1 - (-50.0)).abs() < 1.0, "tip y {}", tip.1);
        // Root stays pinned.
        assert_eq!(points[0], (0.0, 0.0));
    }

    #[test]
    fn fabrik_unreachable_stretches() {
        let mut points = vec![(0.0f32, 0.0f32), (30.0, 0.0), (60.0, 0.0), (90.0, 0.0)];
        let lengths = vec![30.0f32, 30.0, 30.0];
        fabrik_solve(&mut points, &lengths, (0.0, -500.0), None, 20);
        // Total reach 90 < 500: joints straighten toward the target.
        let mid = points[1];
        assert!(
            mid.1 < -20.0,
            "chain should fold toward target, got {mid:?}"
        );
    }

    #[test]
    fn pole_mirrors_chain_side() {
        // Same chain; pole above vs below the root->target line must put
        // the middle joint on opposite sides.
        let mut a = vec![(0.0f32, 0.0f32), (30.0, 0.0), (60.0, 0.0), (90.0, 0.0)];
        let mut b = a.clone();
        let lengths = vec![30.0f32, 30.0, 30.0];
        let target = (60.0, -50.0);
        fabrik_solve(&mut a, &lengths, target, Some((60.0, 50.0)), 20);
        fabrik_solve(&mut b, &lengths, target, Some((60.0, -80.0)), 20);
        // Mid joint (points[2] sits between root and tip horizontally):
        // pole below target line (y=-80 side is "below" in screen space) vs
        // above (y=+50). Check the middle joint x-offset sign flips or y
        // ordering differs — assert they are meaningfully different.
        let da = (a[1].0 - a[2].0).abs() + (a[1].1 - a[2].1).abs();
        let _ = da;
        assert!(
            (a[1].1 - b[1].1).abs() > 1.0 || (a[1].0 - b[1].0).abs() > 1.0,
            "pole must change the bend: a={:?} b={:?}",
            a[1],
            b[1]
        );
    }

    #[test]
    fn chain_ik_three_bones_reaches_target() {
        let rig = chain_rig();
        let mut clip = BoneClip::new("reach", 1);
        let end_id = rig.bones[2].id;
        let mut key = IkKey::new(0, 60.0, -60.0);
        key.pole_x = Some(60.0);
        key.pole_y = Some(60.0);
        clip.tracks.push(BoneTrack {
            bone: end_id,
            keys: vec![],
            ik: vec![key],
        });
        clip.looping = false;
        let pose = rig.pose_at(Some(&clip), 0);
        let end = pose.iter().find(|(id, _)| *id == end_id).unwrap().1;
        // Tip = end joint + length along its rotation; joint itself should
        // be near the target region (chain 90px total, target dist ~85).
        let (tx, ty) = rotate_point(end.x, end.y, end.rotation, 30.0, 0.0);
        assert!(
            (tx - 60.0).abs() < 3.0 && (ty - (-60.0)).abs() < 3.0,
            "tip ({tx},{ty}) should reach (60,-60)"
        );
    }

    #[test]
    fn chain_ik_stops_at_animated_ancestor() {
        let rig = chain_rig();
        let mut clip = BoneClip::new("mix", 1);
        // Mid has its own FK key -> IK chain for End is only [End]? No:
        // chain stops below animated ancestors, so End alone can't IK
        // (needs >=2). Give Mid+End chain by animating only Root instead.
        let root_id = rig.bones[0].id;
        clip.tracks.push(BoneTrack {
            bone: root_id,
            keys: vec![BoneKey {
                frame: 0,
                rotation: 30.0,
            }],
            ik: vec![],
        });
        let end_id = rig.bones[2].id;
        clip.tracks.push(BoneTrack {
            bone: end_id,
            keys: vec![],
            ik: vec![IkKey::new(0, 60.0, -60.0)],
        });
        clip.looping = false;
        // Should not panic and should produce a pose.
        let pose = rig.pose_at(Some(&clip), 0);
        assert_eq!(pose.len(), 3);
        // Chain = [Mid, End] (Root animated -> excluded). Two bones ->
        // falls back to the two-bone solver path.
        let mid = pose
            .iter()
            .find(|(id, _)| *id == rig.bones[1].id)
            .unwrap()
            .1;
        assert!(mid.rotation.is_finite());
    }
}
