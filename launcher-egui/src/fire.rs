//! Embers and flames behind the Home page: the iced launcher's simulation (`ui/fire.rs`), drawn
//! with egui's painter.

use egui::epaint::Mesh;
use egui::{Color32, Painter, Pos2, Rect, TextureId, Vec2, pos2, vec2};

const SEED: u32 = 0x5eed_f1e5;
const FULL_COUNT: f32 = 120.0;
const MAX_SPAWN_PER_TICK: usize = 6;
const FLAME_SHARE: f32 = 0.22;
const RISE: f32 = 0.22;
const EXTRA_LIFE: f32 = 0.7;
/// A flame's rings, outside in: share of the radius and alpha at full life.
const FLAME_RINGS: [(f32, f32); 3] = [(1.0, 0.05), (0.65, 0.07), (0.35, 0.10)];

/// The iced launcher's `intensity_for`, without the splash and the setup the spike leaves out.
pub fn intensity_for(focused: bool, effects: bool) -> f32 {
    match (effects, focused) {
        (false, _) => 0.0,
        (true, false) => 0.3,
        (true, true) => 0.8,
    }
}

struct Xorshift32(u32);

impl Xorshift32 {
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next_f32()
    }
}

struct Particle {
    pos: Pos2,
    vel: Vec2,
    life: f32,
    max_life: f32,
    flame: bool,
    radius: f32,
}

impl Particle {
    fn colour(&self, alpha: f32) -> Color32 {
        let left = (1.0 - self.life / self.max_life).clamp(0.0, 1.0);
        let (r, g, b, a) = if self.flame {
            (0.95, 0.25 + 0.2 * left, 0.05, left)
        } else {
            (1.0, 0.45 + 0.3 * left, 0.12, 0.9 * left)
        };
        Color32::from_rgba_unmultiplied(
            (r * 255.0) as u8,
            (g * 255.0) as u8,
            (b * 255.0) as u8,
            (a * alpha * 255.0) as u8,
        )
    }
}

pub struct Fire {
    particles: Vec<Particle>,
    intensity: f32,
    rng: Xorshift32,
}

impl Default for Fire {
    fn default() -> Self {
        Fire {
            particles: Vec::new(),
            intensity: 0.0,
            rng: Xorshift32(SEED),
        }
    }
}

impl Fire {
    pub fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity.clamp(0.0, 1.0);
    }

    pub fn burning(&self) -> bool {
        self.intensity > 0.0
    }

    fn spawn(&mut self, size: Vec2) {
        let x = self.rng.next_f32() * size.x;
        let line = size.y - RISE * size.y * (x / size.x);
        let y = line + self.rng.range(-6.0, 6.0);
        let flame = self.rng.next_f32() < FLAME_SHARE;
        let (rise, radius, base_life) = if flame {
            (
                self.rng.range(30.0, 80.0),
                self.rng.range(14.0, 34.0),
                self.rng.range(1.2, 2.2),
            )
        } else {
            (
                self.rng.range(60.0, 160.0),
                self.rng.range(1.0, 2.5),
                self.rng.range(2.5, 4.5),
            )
        };
        let drift = self.rng.range(-15.0, 15.0);
        self.particles.push(Particle {
            pos: pos2(x, y),
            vel: vec2(drift, -rise),
            life: 0.0,
            max_life: base_life * (1.0 + EXTRA_LIFE * x / size.x),
            flame,
            radius,
        });
    }

    /// Advances the fire by `dt` seconds in a scene of `size`.
    pub fn tick(&mut self, dt: f32, size: Vec2) {
        if self.intensity <= 0.0 {
            self.particles.clear();
            return;
        }
        for particle in &mut self.particles {
            particle.life += dt;
            particle.pos += particle.vel * dt;
        }
        self.particles
            .retain(|p| p.life < p.max_life && p.pos.y > -p.radius);
        if size.x > 0.0 && size.y > 0.0 {
            let target = (FULL_COUNT * self.intensity) as usize;
            let mut born = 0;
            while self.particles.len() < target && born < MAX_SPAWN_PER_TICK {
                self.spawn(size);
                born += 1;
            }
        }
    }

    /// One textured quad per particle in a single mesh, as the iced shader fire draws them:
    /// `glow` (the soft round sprite) for both, a flame at the summed alpha of its rings.
    pub fn paint(&self, painter: &Painter, origin: Pos2, glow: TextureId) {
        let flame_peak: f32 = FLAME_RINGS.iter().map(|(_, alpha)| alpha).sum();
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let mut mesh = Mesh::with_texture(glow);
        for particle in &self.particles {
            let at = origin + particle.pos.to_vec2();
            // The sprite fades to its edge: an ember's quad is drawn larger to keep its body.
            let (radius, alpha) = if particle.flame {
                (particle.radius, flame_peak)
            } else {
                (particle.radius * 2.0, 1.0)
            };
            mesh.add_rect_with_uv(
                Rect::from_center_size(at, Vec2::splat(2.0 * radius)),
                uv,
                particle.colour(alpha),
            );
        }
        painter.add(mesh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fire_fills_in_and_goes_out() {
        let size = vec2(1000.0, 800.0);
        let mut fire = Fire::default();
        fire.set_intensity(1.0);
        for _ in 0..60 {
            fire.tick(0.033, size);
        }
        assert!(fire.particles.len() > 60);
        fire.set_intensity(0.0);
        fire.tick(0.033, size);
        assert!(fire.particles.is_empty());
    }
}
