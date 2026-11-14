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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IkKey {
    pub frame: u32,
    pub x: f32,
    pub y: f32,
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
}
