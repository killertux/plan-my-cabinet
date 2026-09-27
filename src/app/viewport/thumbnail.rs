//! Deterministic, CPU-offscreen raster of the committed scene. This deliberately
//! has no egui frame, live camera, selection, drag or preview as inputs.
use super::*;

pub(crate) const WIDTH: usize = 192;
pub(crate) const HEIGHT: usize = 120;

pub(crate) fn saved_thumbnail(project: &Project) -> Result<Vec<u8>, &'static str> {
    let selection = Selection::default();
    let mut camera = Camera {
        projection: Projection::Orthographic,
        viewport_aspect: WIDTH as f64 / HEIGHT as f64,
        ..Default::default()
    };
    let bounds = bounds_visible(project, &HashSet::new(), &selection).ok_or("No scene geometry")?;
    camera.frame(bounds, camera.viewport_aspect);
    let (mesh, _) = scene_render::scene_with_faces(project, &camera, &selection, None, None, true);
    let mut pixels = vec![0_u8; WIDTH * HEIGHT * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[236, 232, 225, 255]);
    }
    let mut depths = vec![f64::INFINITY; WIDTH * HEIGHT];
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
    let (_, _, forward) = camera.basis();
    for triangle in mesh.faces.chunks_exact(18) {
        let mut points = [(egui::Pos2::ZERO, 0.0, [0.0_f32; 3]); 3];
        for (vertex, point) in triangle.chunks_exact(6).zip(points.iter_mut()) {
            let world = std::array::from_fn(|i| camera.target[i] + vertex[i] as f64);
            let position = camera.project(world, rect).ok_or("Unprojectable scene")?;
            let depth = camera.distance + dot(std::array::from_fn(|i| vertex[i] as f64), forward);
            if !depth.is_finite() || depth <= 0.0 {
                return Err("Invalid thumbnail depth");
            }
            *point = (position, depth, [vertex[3], vertex[4], vertex[5]]);
        }
        raster_triangle(&mut pixels, &mut depths, points);
    }
    Ok(pixels)
}

fn raster_triangle(pixels: &mut [u8], depths: &mut [f64], v: [(egui::Pos2, f64, [f32; 3]); 3]) {
    let edge = |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| {
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
    };
    let area = edge(v[0].0, v[1].0, v[2].0);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = v
        .iter()
        .map(|p| p.0.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = v
        .iter()
        .map(|p| p.0.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(WIDTH as f32) as usize;
    let min_y = v
        .iter()
        .map(|p| p.0.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_y = v
        .iter()
        .map(|p| p.0.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(HEIGHT as f32) as usize;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            let w = [
                edge(v[1].0, v[2].0, p) / area,
                edge(v[2].0, v[0].0, p) / area,
                edge(v[0].0, v[1].0, p) / area,
            ];
            if w.iter().any(|weight| *weight < -1e-5) {
                continue;
            }
            let depth: f64 = (0..3).map(|i| w[i] as f64 * v[i].1).sum();
            let index = y * WIDTH + x;
            if depth >= depths[index] {
                continue;
            }
            depths[index] = depth;
            for channel in 0..3 {
                let value: f32 = (0..3).map(|i| w[i] * v[i].2[channel]).sum();
                pixels[index * 4 + channel] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::{money::Currency, reference_fixture};

    #[test]
    fn saved_scene_is_deterministic_framed_and_independent_of_live_view() {
        let project = reference_fixture::project();
        let mut live_camera = Camera::reference_baseline();
        live_camera.orbit(100.0, 80.0);
        let before = live_camera.uniform([192, 120], 4000.0);
        let mut selection = Selection::default();
        selection.choose(Some(project.boards[0].id), false);
        selection.hidden.insert(project.boards[1].id);
        let first = saved_thumbnail(&project).unwrap();
        let second = saved_thumbnail(&project).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), WIDTH * HEIGHT * 4);
        assert!(first.chunks_exact(4).any(|p| p != [236, 232, 225, 255]));
        assert_eq!(live_camera.uniform([192, 120], 4000.0), before);
        assert_eq!(selection.active, Some(project.boards[0].id));
        assert!(selection.hidden.contains(&project.boards[1].id));
        assert!(saved_thumbnail(&Project::new("Empty", Currency::Usd)).is_err());
    }
}
