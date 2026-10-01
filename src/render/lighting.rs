//! How the scene is lit: one directional light plus a soft sky term, from
//! each surface's world normal. The CPU rasterizer and the viewport shader use
//! the same formula ([`Light::brightness`] and [`wgsl`]), so pictures and the
//! live view agree. Display only: nothing here is saved in a project.
use serde::{Deserialize, Serialize};

use crate::render::camera::Camera;

/// Always lit, whatever the light: faces turned away never go black.
const AMBIENT: f32 = 0.40;
/// The directional light's share at full facing.
const DIFFUSE: f32 = 0.50;
/// Faces pointing up get a little more light than faces pointing down.
const SKY: f32 = 0.12;
/// With the light off: even shading, with only the sky term to tell tops
/// from sides.
const FLAT: f32 = 0.80;
const FLAT_SKY: f32 = 0.16;
/// Shaded colors move toward this grey instead of toward black.
const SHADOW_LIFT: f32 = 0.12;

/// Where the viewport's light comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LightingMode {
    /// A headlight: the light sits just above and beside the eye, so what you
    /// look at is lit.
    #[default]
    FollowCamera,
    /// The light stays put in the room while the camera moves.
    Fixed,
    /// No directional light: even, soft shading.
    Off,
}

impl LightingMode {
    pub const ALL: [Self; 3] = [Self::FollowCamera, Self::Fixed, Self::Off];

    /// The next mode, for a button that cycles them.
    pub fn next(self) -> Self {
        match self {
            Self::FollowCamera => Self::Fixed,
            Self::Fixed => Self::Off,
            Self::Off => Self::FollowCamera,
        }
    }
}

/// The user's lighting choice, kept in the machine-local preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LightingPreference {
    pub mode: LightingMode,
    /// Fixed light: degrees around the vertical, 0 from the front (−Y),
    /// growing toward the right (+X).
    pub azimuth_deg: i16,
    /// Fixed light: degrees above the floor.
    pub elevation_deg: i16,
}

impl Default for LightingPreference {
    fn default() -> Self {
        Self {
            mode: LightingMode::FollowCamera,
            azimuth_deg: 35,
            elevation_deg: 50,
        }
    }
}

pub const ELEVATION_RANGE: std::ops::RangeInclusive<i16> = -30..=90;

impl LightingPreference {
    /// The light to draw a view from `camera` with.
    pub fn light(&self, camera: &Camera) -> Light {
        match self.mode {
            LightingMode::FollowCamera => Light::headlight(camera),
            LightingMode::Fixed => Light::from_angles(self.azimuth_deg, self.elevation_deg),
            LightingMode::Off => Light::OFF,
        }
    }

    /// Fix the light where the headlight is now, so it stays there.
    pub fn fix_at_camera(&mut self, camera: &Camera) {
        self.fix_at(Light::headlight(camera));
    }

    /// Fix the light at `light`'s direction.
    pub fn fix_at(&mut self, light: Light) {
        let (azimuth, elevation) = light.angles();
        self.mode = LightingMode::Fixed;
        self.azimuth_deg = azimuth;
        self.elevation_deg = elevation;
    }

    /// Keep the angles in range (a hand-edited preferences file).
    pub fn normalized(mut self) -> Self {
        self.azimuth_deg = self.azimuth_deg.rem_euclid(360);
        self.elevation_deg = self
            .elevation_deg
            .clamp(*ELEVATION_RANGE.start(), *ELEVATION_RANGE.end());
        self
    }
}

/// One directional light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// Unit vector from the surface toward the light, in world axes (Z up).
    pub direction: [f32; 3],
    /// False: no directional light, only the even shading.
    pub enabled: bool,
}

impl Light {
    pub const OFF: Self = Self {
        direction: [0.0, 0.0, 1.0],
        enabled: false,
    };

    /// The fixed light for pictures (PDF, thumbnails, agent pictures):
    /// above, in front and to the right, the same for everyone.
    pub fn studio() -> Self {
        Self::toward([0.35, -0.5, 0.8])
    }

    /// Just above and to the right of the eye, pointing where it looks.
    pub fn headlight(camera: &Camera) -> Self {
        let (right, up, forward) = camera.basis();
        Self::toward(std::array::from_fn(|i| {
            (-forward[i] + 0.35 * up[i] + 0.25 * right[i]) as f32
        }))
    }

    /// A light at `azimuth` degrees around (0 = front, 90 = right) and
    /// `elevation` degrees up.
    pub fn from_angles(azimuth_deg: i16, elevation_deg: i16) -> Self {
        let (az, el) = (
            f32::from(azimuth_deg).to_radians(),
            f32::from(elevation_deg).to_radians(),
        );
        Self::toward([az.sin() * el.cos(), -az.cos() * el.cos(), el.sin()])
    }

    /// The light's direction as whole degrees: azimuth in 0..360 and
    /// elevation.
    pub fn angles(&self) -> (i16, i16) {
        let [x, y, z] = self.direction;
        let azimuth = x.atan2(-y).to_degrees().round() as i16;
        let elevation = z.clamp(-1.0, 1.0).asin().to_degrees().round() as i16;
        (azimuth.rem_euclid(360), elevation)
    }

    fn toward(v: [f32; 3]) -> Self {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        Self {
            direction: if len > 1e-9 {
                v.map(|c| c / len)
            } else {
                [0.0, 0.0, 1.0]
            },
            enabled: true,
        }
    }

    /// How much light a surface with this unit normal gets: about 0.4 facing
    /// away to 1.0 facing the light.
    pub fn brightness(&self, normal: [f32; 3]) -> f32 {
        let sky = 0.5 + 0.5 * normal[2];
        if self.enabled {
            let facing = (0..3).map(|i| normal[i] * self.direction[i]).sum::<f32>();
            AMBIENT + DIFFUSE * facing.max(0.0) + SKY * sky
        } else {
            FLAT + FLAT_SKY * sky
        }
    }

    /// The four floats the shader's `light` uniform takes.
    pub fn uniform(&self) -> [f32; 4] {
        let [x, y, z] = self.direction;
        [x, y, z, if self.enabled { 1.0 } else { 0.0 }]
    }
}

/// A base color under the given brightness.
pub fn shade(color: [f32; 3], brightness: f32) -> [f32; 3] {
    color.map(|c| (c * brightness + SHADOW_LIFT * (1.0 - brightness)).clamp(0.0, 1.0))
}

/// The same formula as [`Light::brightness`] and [`shade`], as WGSL
/// functions `brightness(normal, light)` and `shade(color, b)`.
pub fn wgsl() -> String {
    format!(
        "fn brightness(n: vec3<f32>, light: vec4<f32>) -> f32 {{
    let sky = 0.5 + 0.5 * n.z;
    if (light.w > 0.5) {{
        return {AMBIENT:?} + {DIFFUSE:?} * max(dot(n, light.xyz), 0.0) + {SKY:?} * sky;
    }}
    return {FLAT:?} + {FLAT_SKY:?} * sky;
}}
fn shade(color: vec3<f32>, b: f32) -> vec3<f32> {{
    return clamp(color * b + vec3<f32>({SHADOW_LIFT:?} * (1.0 - b)), vec3<f32>(0.0), vec3<f32>(1.0));
}}
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::camera::Camera;

    fn unit(v: [f32; 3]) -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|c| c / len)
    }

    #[test]
    fn facing_the_light_is_brightest_and_nothing_goes_black() {
        let light = Light::studio();
        let lit = light.brightness(light.direction);
        let away = light.brightness(light.direction.map(|c| -c));
        assert!(lit > 0.95, "{lit}");
        assert!(away >= AMBIENT - 1e-6, "{away}");
        for n in [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]] {
            let b = light.brightness(n);
            assert!(b <= lit && b >= AMBIENT, "{n:?} {b}");
        }
    }

    #[test]
    fn with_the_light_off_shading_is_even() {
        let side = Light::OFF.brightness([1.0, 0.0, 0.0]);
        assert_eq!(side, Light::OFF.brightness([0.0, -1.0, 0.0]));
        assert!(Light::OFF.brightness([0.0, 0.0, 1.0]) > side);
    }

    #[test]
    fn the_headlight_lights_what_the_camera_looks_at() {
        let mut camera = Camera::default();
        for (yaw, pitch) in [(0.3, 0.4), (2.0, -0.2), (4.5, 1.2)] {
            camera.yaw = yaw;
            camera.pitch = pitch;
            let light = Light::headlight(&camera);
            let (_, _, forward) = camera.basis();
            let toward_eye = forward.map(|c| -c as f32);
            // A face looking straight at the eye is lit nearly fully, and more
            // than any face turned sideways.
            let front = light.brightness(toward_eye);
            for n in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
                let n = unit(n);
                if (0..3).map(|i| n[i] * toward_eye[i]).sum::<f32>() < 0.9 {
                    assert!(light.brightness(n) < front, "{yaw} {pitch} {n:?}");
                }
            }
            assert!(front > 0.85, "{front}");
        }
    }

    #[test]
    fn angles_round_trip_and_fixing_at_the_camera_keeps_the_light_where_it_was() {
        for (az, el) in [(0, 0), (35, 50), (180, 10), (270, 80), (359, -20)] {
            assert_eq!(Light::from_angles(az, el).angles(), (az, el));
        }
        let mut camera = Camera {
            yaw: 1.1,
            pitch: 0.5,
            ..Default::default()
        };
        let mut preference = LightingPreference::default();
        let before = preference.light(&camera);
        preference.fix_at_camera(&camera);
        assert_eq!(preference.mode, LightingMode::Fixed);
        let after = preference.light(&camera);
        let dot: f32 = (0..3)
            .map(|i| before.direction[i] * after.direction[i])
            .sum();
        assert!(dot > 0.999, "{dot}");
        // Moving the camera no longer moves the light.
        camera.yaw = 3.0;
        assert_eq!(preference.light(&camera), after);
    }

    #[test]
    fn shading_lifts_toward_grey_and_the_shader_uses_the_same_numbers() {
        assert_eq!(shade([1.0, 0.5, 0.0], 1.0), [1.0, 0.5, 0.0]);
        let dark = shade([0.0; 3], 0.4);
        assert!(dark[0] > 0.0);
        let source = wgsl();
        for value in [AMBIENT, DIFFUSE, SKY, FLAT, FLAT_SKY, SHADOW_LIFT] {
            assert!(source.contains(&format!("{value:?}")), "{value}");
        }
    }

    #[test]
    fn old_preferences_without_angles_use_the_defaults() {
        let parsed: LightingPreference = serde_json::from_str(r#"{"mode":"fixed"}"#).unwrap();
        assert_eq!(parsed.mode, LightingMode::Fixed);
        assert_eq!(parsed.azimuth_deg, 35);
        assert_eq!(
            LightingPreference {
                azimuth_deg: -10,
                elevation_deg: 120,
                ..Default::default()
            }
            .normalized(),
            LightingPreference {
                azimuth_deg: 350,
                elevation_deg: 90,
                ..Default::default()
            }
        );
    }
}
