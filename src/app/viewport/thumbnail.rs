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
    let style = plan_my_cabinet::render::raster::SceneStyle {
        width: WIDTH,
        height: HEIGHT,
        show_shadow: false,
        supersample: 1,
        ..Default::default()
    };
    plan_my_cabinet::render::raster::render_scene(project, &camera, &selection, &style)
        .map(|raster| raster.rgba)
        .map_err(|_| "Unrenderable scene")
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
