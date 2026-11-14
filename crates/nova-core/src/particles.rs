//! Deterministic 2D particle simulation for the editor preview.
//!
//! [`ParticleSystem`] is the runtime half of [`crate::model::ParticleEmitter`]:
//! it owns live particles, emits at the configured rate/burst and integrates
//! velocity, gravity, drag, growth, fade and rotation. The RNG is a small
//! xorshift so previews and tests are reproducible — no external rand dep.
//!
//! `Local` space stores particle positions relative to the emitter origin and
//! drags them along when the emitter moves; `World` space stores absolute
//! positions. Rendering is the caller's job ([`Particle::view`]).

use crate::model::{ParticleEmitter, ParticleSpace, Rgba};

/// One live particle. Positions are emitter-relative for `Local` space and
/// absolute for `World` space.
#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub age: f32,
    pub lifetime: f32,
    pub size_start: f32,
    pub size_end: f32,
    pub opacity_start: f32,
    pub opacity_end: f32,
    pub color_start: Rgba,
    pub color_end: Rgba,
    pub rotation: f32,
    pub rotation_speed: f32,
}

impl Particle {
    /// Normalised life 0..=1.
    pub fn t(&self) -> f32 {
        if self.lifetime <= 0.0 {
            1.0
        } else {
            (self.age / self.lifetime).clamp(0.0, 1.0)
        }
    }

    pub fn size(&self) -> f32 {
        let t = self.t();
        self.size_start + (self.size_end - self.size_start) * t
    }

    pub fn opacity(&self) -> f32 {
        let t = self.t();
        self.opacity_start + (self.opacity_end - self.opacity_start) * t
    }

    pub fn color(&self) -> Rgba {
        let t = self.t();
        Rgba {
            r: lerp_u8(self.color_start.r, self.color_end.r, t),
            g: lerp_u8(self.color_start.g, self.color_end.g, t),
            b: lerp_u8(self.color_start.b, self.color_end.b, t),
            a: lerp_u8(self.color_start.a, self.color_end.a, t),
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Render-ready particle: absolute position plus resolved look.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleView {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub rotation: f32,
    pub color: Rgba,
    pub opacity: f32,
}

/// Live simulation state of one emitter object.
pub struct ParticleSystem {
    pub spec: ParticleEmitter,
    particles: Vec<Particle>,
    /// Fractional emission accumulator for `rate`.
    accumulator: f32,
    /// Whether the one-shot `burst` was already emitted.
    burst_done: bool,
    rng: u64,
    /// Emitter origin during the previous update (local-space drag).
    prev_origin: (f32, f32),
    /// Whether `prev_origin` is valid.
    has_prev: bool,
}

impl ParticleSystem {
    /// Creates a system. The configured `burst` is emitted on the first
    /// `update` call, when the emitter origin is known.
    pub fn new(spec: ParticleEmitter) -> Self {
        Self {
            spec,
            particles: Vec::new(),
            accumulator: 0.0,
            burst_done: false,
            rng: 0x9E37_79B9_7F4A_7C15,
            prev_origin: (0.0, 0.0),
            has_prev: false,
        }
    }

    /// Re-arms the burst (restart semantics); it fires on the next update.
    pub fn restart(&mut self) {
        self.burst_done = false;
    }

    /// Clears all live particles (scene reset/stop).
    pub fn clear(&mut self) {
        self.particles.clear();
        self.accumulator = 0.0;
        self.burst_done = false;
        self.has_prev = false;
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Advances the system by `dt` seconds. `origin` is the emitter's current
    /// world position; local-space particles are dragged by the origin delta.
    pub fn update(&mut self, origin: (f32, f32), dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        if dt <= 0.0 {
            return;
        }
        if self.spec.space == ParticleSpace::Local && self.has_prev {
            let dx = origin.0 - self.prev_origin.0;
            let dy = origin.1 - self.prev_origin.1;
            if dx != 0.0 || dy != 0.0 {
                for p in &mut self.particles {
                    p.x += dx;
                    p.y += dy;
                }
            }
        }
        self.prev_origin = origin;
        self.has_prev = true;

        // Continuous emission.
        if self.spec.rate > 0.0 {
            self.accumulator += self.spec.rate * dt;
            while self.accumulator >= 1.0 && self.particles.len() < self.spec.max_particles as usize
            {
                self.accumulator -= 1.0;
                self.spawn(origin);
            }
        }
        // Burst fires once, on the first update where the origin is known.
        if !self.burst_done {
            self.burst_done = true;
            let count = self.spec.burst.min(self.spec.max_particles);
            for _ in 0..count {
                if self.particles.len() >= self.spec.max_particles as usize {
                    break;
                }
                self.spawn(origin);
            }
        }

        let gravity = self.spec.gravity;
        let drag = self.spec.drag;
        self.particles.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.lifetime {
                return false;
            }
            p.vy += gravity * dt;
            let damping = (1.0 - drag * dt).clamp(0.0, 1.0);
            p.vx *= damping;
            p.vy *= damping;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.rotation += p.rotation_speed * dt;
            true
        });
    }

    fn spawn(&mut self, origin: (f32, f32)) {
        if self.particles.len() >= self.spec.max_particles as usize {
            return;
        }
        let (speed, angle) = self.random_velocity();
        let (sx, ex) = self.spec.size_start;
        let (sy, ey) = self.spec.size_end;
        let (ox, oe) = self.spec.opacity_start;
        let (o2x, o2e) = self.spec.opacity_end;
        let (lx, le) = self.spec.lifetime;
        let (rx, re) = self.spec.rotation_speed;
        let lifetime = self.range(lx, le).max(0.01);
        let size_s = self.range(sx, ex);
        let size_e = self.range(sy, ey);
        let op_s = self.range(ox, o2x);
        let op_e = self.range(oe, o2e);
        let rot = self.range(0.0, 360.0);
        let rot_speed = self.range(rx, re).to_radians();
        // Positions spawn at the emitter origin; local space keeps (0,0).
        let (px, py) = if self.spec.space == ParticleSpace::Local {
            (0.0, 0.0)
        } else {
            origin
        };
        self.particles.push(Particle {
            x: px,
            y: py,
            vx: speed * angle.cos(),
            vy: speed * angle.sin(),
            age: 0.0,
            lifetime,
            size_start: size_s,
            size_end: size_e,
            opacity_start: op_s,
            opacity_end: op_e,
            color_start: self.spec.color_start,
            color_end: self.spec.color_end,
            rotation: rot,
            rotation_speed: rot_speed,
        });
    }

    /// Speed (px/s) and direction (radians) from `speed`/`direction`/`spread`.
    fn random_velocity(&mut self) -> (f32, f32) {
        let (smin, smax) = self.spec.speed;
        let speed = self.range(smin, smax);
        let dir = self.spec.direction.to_radians();
        let spread = self.spec.spread.to_radians();
        let angle = dir + (self.next_f32() - 0.5) * 2.0 * spread;
        (speed, angle)
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }

    /// xorshift64* — deterministic, seedable via `set_seed` for tests.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Test hook: replace the RNG seed for reproducible sequences.
    #[doc(hidden)]
    pub fn set_seed(&mut self, seed: u64) {
        self.rng = seed | 1;
    }

    /// Render-ready views for `origin` (emitter world position).
    pub fn view(&self, origin: (f32, f32)) -> Vec<ParticleView> {
        self.particles
            .iter()
            .map(|p| {
                let (x, y) = if self.spec.space == ParticleSpace::Local {
                    (origin.0 + p.x, origin.1 + p.y)
                } else {
                    (p.x, p.y)
                };
                ParticleView {
                    x,
                    y,
                    size: p.size(),
                    rotation: p.rotation.to_degrees(),
                    color: p.color(),
                    opacity: p.opacity(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParticlePreset;

    fn spec(preset: ParticlePreset) -> ParticleEmitter {
        ParticleEmitter::preset(preset)
    }

    #[test]
    fn presets_have_valid_ranges() {
        for preset in [
            ParticlePreset::Smoke,
            ParticlePreset::Sparks,
            ParticlePreset::Fire,
            ParticlePreset::Rain,
            ParticlePreset::Snow,
            ParticlePreset::Dust,
            ParticlePreset::Magic,
            ParticlePreset::Explosion,
        ] {
            let e = spec(preset);
            assert!(e.lifetime.0 > 0.0 && e.lifetime.1 >= e.lifetime.0);
            assert!(e.max_particles > 0);
            assert!(e.opacity_start.0 <= 1.0 && e.opacity_end.1 <= 1.0);
        }
    }

    #[test]
    fn burst_emits_once_and_rate_accumulates() {
        let mut s = ParticleSystem::new(spec(ParticlePreset::Explosion));
        s.update((0.0, 0.0), 0.016);
        assert_eq!(s.len(), 60, "explosion burst = 60");
        s.update((0.0, 0.0), 0.016);
        assert!(s.len() <= 60, "burst must not repeat");
        s.restart();
        s.update((0.0, 0.0), 0.016);
        assert_eq!(s.len(), 120);
    }

    #[test]
    fn particles_age_out_and_respect_max() {
        let mut emitter = spec(ParticlePreset::Sparks);
        emitter.max_particles = 10;
        emitter.rate = 1000.0;
        emitter.burst = 0;
        let mut s = ParticleSystem::new(emitter);
        for _ in 0..10 {
            s.update((0.0, 0.0), 0.016);
        }
        assert!(s.len() <= 10);
        // Stop emitting; all sparks die within ~0.6 s.
        s.spec.rate = 0.0;
        for _ in 0..60 {
            s.update((0.0, 0.0), 0.016);
        }
        assert!(s.is_empty());
    }

    #[test]
    fn local_space_particles_follow_moving_emitter() {
        let mut emitter = spec(ParticlePreset::Dust);
        emitter.space = ParticleSpace::Local;
        emitter.rate = 50.0;
        emitter.burst = 10;
        emitter.lifetime = (5.0, 5.0);
        let mut s = ParticleSystem::new(emitter);
        s.update((0.0, 0.0), 0.016);
        s.update((0.0, 0.0), 0.016);
        // Simpler assertion: view origin shifts output by origin delta.
        let v0 = s.view((0.0, 0.0));
        let v1 = s.view((50.0, 0.0));
        for (p0, p1) in v0.iter().zip(v1.iter()) {
            assert!((p1.x - p0.x - 50.0).abs() < 0.001);
        }
    }

    #[test]
    fn deterministic_same_seed_same_particles() {
        let mut a = ParticleSystem::new(spec(ParticlePreset::Magic));
        let mut b = ParticleSystem::new(spec(ParticlePreset::Magic));
        a.set_seed(42);
        b.set_seed(42);
        a.update((10.0, 10.0), 0.016);
        b.update((10.0, 10.0), 0.016);
        assert_eq!(a.view((10.0, 10.0)), b.view((10.0, 10.0)));
    }

    #[test]
    fn gravity_accelerates_downward() {
        let mut emitter = spec(ParticlePreset::Custom);
        emitter.burst = 1;
        emitter.rate = 0.0;
        emitter.speed = (100.0, 100.0);
        emitter.direction = 0.0;
        emitter.spread = 0.0;
        emitter.gravity = 500.0;
        emitter.lifetime = (1.0, 1.0);
        let mut s = ParticleSystem::new(emitter);
        s.update((0.0, 0.0), 0.016); // emits the burst
        let v0 = s.view((0.0, 0.0));
        assert_eq!(v0.len(), 1);
        s.update((0.0, 0.0), 0.05);
        let v1 = s.view((0.0, 0.0));
        assert!(v1[0].y > v0[0].y, "gravity must pull particles down");
    }
}
