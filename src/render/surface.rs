//! Raw sheet surfaces: the fibre of MDF, the chips of MDP, plywood plies and
//! wood grain. Each is a small tileable grayscale detail map, generated here
//! (no image files) and multiplied into a base color, so colors stay data
//! and the maps only add texture. The CPU rasterizer samples them with
//! [`sample`]; the viewport uploads the same bytes ([`mip_levels`]) and reads
//! them with the [`wgsl`] function.
use std::sync::OnceLock;

use crate::domain::MaterialKind;

/// Texels per side of every map.
pub const SIZE: usize = 128;
/// A texel byte `b` stands for `DETAIL_LOW + DETAIL_SPAN * b / 255`.
const DETAIL_LOW: f32 = 0.7;
const DETAIL_SPAN: f32 = 0.5;

/// A raw surface's texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    MdfFibre,
    MdpChips,
    HdfFibre,
    PlywoodPlies,
    WoodGrain,
}

impl Surface {
    pub const ALL: [Self; 5] = [
        Self::MdfFibre,
        Self::MdpChips,
        Self::HdfFibre,
        Self::PlywoodPlies,
        Self::WoodGrain,
    ];

    /// The layer in the texture array, and the value a vertex carries.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: f32) -> Option<Self> {
        (index >= 0.0)
            .then(|| Self::ALL.get(index as usize).copied())
            .flatten()
    }

    /// Millimetres one tile of the map covers along U and V. Plies and grain
    /// run along U.
    pub fn tile_mm(self) -> [f32; 2] {
        match self {
            Self::MdfFibre => [60.0, 60.0],
            Self::MdpChips => [90.0, 90.0],
            Self::HdfFibre => [50.0, 50.0],
            Self::PlywoodPlies => [80.0, 24.0],
            Self::WoodGrain => [400.0, 120.0],
        }
    }

    /// The raw edge of a sheet of this kind, if it shows one.
    pub fn edge(kind: MaterialKind) -> Option<Self> {
        match kind {
            MaterialKind::Mdf => Some(Self::MdfFibre),
            MaterialKind::Mdp => Some(Self::MdpChips),
            MaterialKind::Hdf => Some(Self::HdfFibre),
            MaterialKind::Plywood => Some(Self::PlywoodPlies),
            MaterialKind::SolidWood => Some(Self::WoodGrain),
            MaterialKind::Other => None,
        }
    }

    /// An uncoated broad face of a sheet of this kind.
    pub fn face(kind: MaterialKind) -> Option<Self> {
        match kind {
            MaterialKind::Plywood => Some(Self::WoodGrain),
            other => Self::edge(other),
        }
    }
}

/// The color of a sheet's raw core: what an uncoated face or an unbanded edge
/// shows. Fixed per kind; `None` for kinds drawn in their material color.
pub fn core_color(kind: MaterialKind) -> Option<[f32; 3]> {
    match kind {
        MaterialKind::Mdf => Some([0.78, 0.64, 0.47]),
        MaterialKind::Mdp => Some([0.82, 0.71, 0.54]),
        MaterialKind::Hdf => Some([0.45, 0.33, 0.23]),
        MaterialKind::Plywood => Some([0.86, 0.76, 0.58]),
        MaterialKind::SolidWood | MaterialKind::Other => None,
    }
}

fn maps() -> &'static [Vec<u8>; 5] {
    static MAPS: OnceLock<[Vec<u8>; 5]> = OnceLock::new();
    MAPS.get_or_init(|| Surface::ALL.map(generate))
}

/// The full-size map, row by row (V down the rows).
pub fn texels(surface: Surface) -> &'static [u8] {
    &maps()[surface.index()]
}

fn decode(byte: u8) -> f32 {
    DETAIL_LOW + DETAIL_SPAN * f32::from(byte) / 255.0
}

/// The detail factor at a point of a face, in millimetres along U and V.
/// Bilinear and wrapping, with texel centres where a GPU sampler puts them.
pub fn sample(surface: Surface, uv_mm: [f32; 2]) -> f32 {
    let tile = surface.tile_mm();
    let texels = texels(surface);
    let at = |axis: usize| {
        let t = (uv_mm[axis] / tile[axis]).rem_euclid(1.0) * SIZE as f32 - 0.5;
        let base = t.floor();
        let i0 = (base as i64).rem_euclid(SIZE as i64) as usize;
        (i0, (i0 + 1) % SIZE, t - base)
    };
    let (x0, x1, fx) = at(0);
    let (y0, y1, fy) = at(1);
    let texel = |x: usize, y: usize| decode(texels[y * SIZE + x]);
    let top = texel(x0, y0) * (1.0 - fx) + texel(x1, y0) * fx;
    let bottom = texel(x0, y1) * (1.0 - fx) + texel(x1, y1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Every mip level of a map, largest first, for the GPU: each level averages
/// 2×2 texels of the one before, so far surfaces don't shimmer.
pub fn mip_levels(surface: Surface) -> Vec<Vec<u8>> {
    let mut levels = vec![texels(surface).to_vec()];
    let mut size = SIZE;
    while size > 1 {
        let previous = levels.last().expect("starts with the full map");
        let half = size / 2;
        let next = (0..half * half)
            .map(|i| {
                let (x, y) = (i % half * 2, i / half * 2);
                let sum: u32 = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)]
                    .iter()
                    .map(|(x, y)| u32::from(previous[y * size + x]))
                    .sum();
                ((sum + 2) / 4) as u8
            })
            .collect();
        levels.push(next);
        size = half;
    }
    levels
}

/// The shader side of [`sample`]: `detail(surface, uv_mm)` from a
/// `texture_2d_array<f32>` named `details` and a repeating `detail_sampler`.
/// A negative `surface` is a smooth face (1.0).
pub fn wgsl() -> String {
    let tiles: Vec<String> = Surface::ALL
        .iter()
        .map(|s| {
            let [u, v] = s.tile_mm();
            format!("vec2<f32>({u:?}, {v:?})")
        })
        .collect();
    format!(
        "fn detail(surface: f32, uv_mm: vec2<f32>) -> f32 {{
    var tiles = array<vec2<f32>, {count}>({tiles});
    let layer = clamp(i32(surface), 0, {last});
    let texel = textureSample(details, detail_sampler, uv_mm / tiles[layer], layer).r;
    return select(1.0, {DETAIL_LOW:?} + {DETAIL_SPAN:?} * texel, surface >= 0.0);
}}
",
        count = Surface::ALL.len(),
        last = Surface::ALL.len() - 1,
        tiles = tiles.join(", "),
    )
}

/// A hash of a lattice point to 0..1, stable across platforms.
fn hash(x: i64, y: i64, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x8da6_b343)
        .wrapping_add((y as u32).wrapping_mul(0xd816_3841))
        .wrapping_add(seed.wrapping_mul(0xcb1a_b31f));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Smooth value noise that repeats every `cells` lattice cells across the
/// map, so the map tiles.
fn noise(x: f32, y: f32, cells: usize, seed: u32) -> f32 {
    let scale = cells as f32 / SIZE as f32;
    let (fx, fy) = (x * scale, y * scale);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(tx), smooth(ty));
    let cells = cells as i64;
    let at = |dx: i64, dy: i64| {
        hash(
            (x0 as i64 + dx).rem_euclid(cells),
            (y0 as i64 + dy).rem_euclid(cells),
            seed,
        )
    };
    let top = at(0, 0) * (1.0 - sx) + at(1, 0) * sx;
    let bottom = at(0, 1) * (1.0 - sx) + at(1, 1) * sx;
    top * (1.0 - sy) + bottom * sy
}

/// Noise centred on zero, roughly −1..1.
fn signed(x: f32, y: f32, cells: usize, seed: u32) -> f32 {
    noise(x, y, cells, seed) * 2.0 - 1.0
}

/// Wood chips: a jittered cell pattern (tiling), each chip its own shade with
/// a darker gap where two chips meet.
fn chips(x: f32, y: f32, cells: i64, seed: u32) -> f32 {
    let size = SIZE as f32 / cells as f32;
    let (cx, cy) = ((x / size).floor() as i64, (y / size).floor() as i64);
    let mut nearest = (f32::INFINITY, 0.0_f32);
    let mut second = f32::INFINITY;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (gx, gy) = (cx + dx, cy + dy);
            let (wx, wy) = (gx.rem_euclid(cells), gy.rem_euclid(cells));
            let px = (gx as f32 + 0.15 + 0.7 * hash(wx, wy, seed)) * size;
            // Chips are longer than wide.
            let py = (gy as f32 + 0.15 + 0.7 * hash(wx, wy, seed + 1)) * size;
            let d = ((px - x) * 0.6).powi(2) + (py - y).powi(2);
            if d < nearest.0 {
                second = nearest.0;
                nearest = (d, hash(wx, wy, seed + 2));
            } else if d < second {
                second = d;
            }
        }
    }
    let gap = (second.sqrt() - nearest.0.sqrt()).clamp(0.0, 1.5) / 1.5;
    (nearest.1 - 0.5) * 0.24 - (1.0 - gap) * 0.10
}

fn generate(surface: Surface) -> Vec<u8> {
    let value = |x: f32, y: f32| -> f32 {
        match surface {
            Surface::MdfFibre => {
                1.0 + 0.035 * signed(x, y, 8, 11)
                    + 0.03 * signed(x, y, 32, 12)
                    + 0.025 * signed(x, y, 128, 13)
            }
            Surface::HdfFibre => {
                1.0 + 0.025 * signed(x, y, 8, 21)
                    + 0.02 * signed(x, y, 32, 22)
                    + 0.02 * signed(x, y, 128, 23)
            }
            Surface::MdpChips => 1.0 + chips(x, y, 16, 31) + 0.03 * signed(x, y, 64, 34),
            Surface::PlywoodPlies => {
                // Sixteen plies across V, alternating in tone, with a dark
                // glue line between them.
                let ply = SIZE as f32 / 16.0;
                let index = (y / ply).floor() as i64;
                let within = y - index as f32 * ply;
                let tone = if index % 2 == 0 { 0.06 } else { -0.05 };
                let glue = if within < 1.0 { -0.12 } else { 0.0 };
                1.0 + tone + glue + 0.03 * signed(x, y, 16, 41) + 0.02 * signed(x, y, 128, 42)
            }
            Surface::WoodGrain => {
                // Rings run along U; noise bends them.
                let rings = 9.0;
                let phase = y / SIZE as f32 * rings + 0.45 * noise(x, y, 4, 51);
                let ring = (phase * std::f32::consts::TAU).sin();
                1.0 + 0.06 * ring * ring.abs() + 0.025 * signed(x, y, 32, 52)
                    - 0.03 * noise(x, y, 128, 53)
            }
        }
    };
    (0..SIZE * SIZE)
        .map(|i| {
            let (x, y) = ((i % SIZE) as f32 + 0.5, (i / SIZE) as f32 + 0.5);
            let t = (value(x, y) - DETAIL_LOW) / DETAIL_SPAN;
            (t.clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_are_deterministic_textured_and_near_one() {
        for surface in Surface::ALL {
            let texels = texels(surface);
            assert_eq!(texels, generate(surface).as_slice());
            let values: Vec<f32> = texels.iter().map(|b| decode(*b)).collect();
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            let (low, high) = values
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(*v), hi.max(*v))
                });
            assert!((0.9..=1.06).contains(&mean), "{surface:?} {mean}");
            assert!(high - low > 0.05, "{surface:?} is flat");
            assert!(low > DETAIL_LOW && high < DETAIL_LOW + DETAIL_SPAN);
        }
    }

    #[test]
    fn maps_tile_without_a_seam() {
        for surface in Surface::ALL {
            let tile = surface.tile_mm();
            // Across the wrap, neighbouring samples differ no more than
            // neighbours inside the tile do.
            let step = tile[0] / SIZE as f32;
            let mut inside = 0.0_f32;
            let mut seam = 0.0_f32;
            for row in 0..SIZE {
                let v = (row as f32 + 0.5) * tile[1] / SIZE as f32;
                for col in 1..SIZE {
                    let u = col as f32 * step;
                    inside = inside
                        .max((sample(surface, [u, v]) - sample(surface, [u - step, v])).abs());
                }
                seam = seam.max(
                    (sample(surface, [tile[0], v]) - sample(surface, [tile[0] - step, v])).abs(),
                );
            }
            assert!(seam <= inside + 1e-3, "{surface:?}: seam {seam} > {inside}");
            let (here, there) = (
                sample(surface, [3.0, 7.0]),
                sample(surface, [3.0 + tile[0], 7.0 - 2.0 * tile[1]]),
            );
            assert!((here - there).abs() < 1e-3, "{surface:?} {here} {there}");
        }
    }

    #[test]
    fn mips_halve_down_to_one_texel_and_the_shader_knows_every_tile() {
        let levels = mip_levels(Surface::MdpChips);
        assert_eq!(levels.len(), 8);
        assert_eq!(levels[0].len(), SIZE * SIZE);
        assert_eq!(levels.last().unwrap().len(), 1);
        let source = wgsl();
        for surface in Surface::ALL {
            let [u, v] = surface.tile_mm();
            assert!(source.contains(&format!("vec2<f32>({u:?}, {v:?})")));
        }
        assert_eq!(Surface::from_index(-1.0), None);
        assert_eq!(Surface::from_index(1.0), Some(Surface::MdpChips));
    }

    #[test]
    fn every_kind_but_other_has_a_raw_look() {
        for kind in MaterialKind::ALL {
            let raw = Surface::edge(kind).is_some();
            assert_eq!(raw, kind != MaterialKind::Other, "{kind:?}");
        }
        assert_eq!(
            Surface::face(MaterialKind::Plywood),
            Some(Surface::WoodGrain)
        );
        assert!(core_color(MaterialKind::Other).is_none());
    }
}
