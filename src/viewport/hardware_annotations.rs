//! Paint-only hardware references. No cutting or drilling primitives enter the scene mesh.
use super::*;
use plan_my_cabinet::domain::{BoardEdge, BoardFace, HingeInstallation};
use plan_my_cabinet::hinge_installation::{self, InstallationIssue};
use plan_my_cabinet::units::Pose;

#[derive(Debug)]
struct Guide {
    id: Uuid,
    axis: Option<[[f64; 3]; 2]>,
    cup: Option<[f64; 3]>,
    plate: Option<[[f64; 3]; 2]>,
    issues: Vec<InstallationIssue>,
    unavailable: bool,
}

fn mm(value: i128) -> f64 {
    value as f64 / 1000.0
}

fn world(pose: Pose, local_um: [i128; 3]) -> Option<[f64; 3]> {
    pose.transform_point(local_um.map(mm)).ok()
}

fn pose_for(project: &Project, board: &Board, poses: Option<&HashMap<Uuid, Pose>>) -> Option<Pose> {
    poses
        .and_then(|poses| poses.get(&board.id).copied())
        .or_else(|| world_pose(project, board))
}

/// Recompute from committed diagnostics every frame; a changed board never inherits old
/// validated references. The axis is an edge/face datum on the moving board, not a bore.
fn guide(
    project: &Project,
    selection: &Selection,
    installation: &HingeInstallation,
    poses: Option<&HashMap<Uuid, Pose>>,
    relationship: bool,
) -> Guide {
    let status = hinge_installation::diagnose(project, installation);
    let door = project
        .boards
        .iter()
        .find(|b| b.id == installation.door_board_id);
    let mount = project
        .boards
        .iter()
        .find(|b| b.id == installation.mounting_board_id);
    let door_pose = door.and_then(|b| pose_for(project, b, poses));
    let mount_pose = mount.and_then(|b| pose_for(project, b, poses));
    let door_visible = selection.visible(project, installation.door_board_id);
    let mount_visible = selection.visible(project, installation.mounting_board_id);
    let axis = if relationship && door_visible {
        door.zip(door_pose).and_then(|(board, pose)| {
            let x = match installation.side.door_edge {
                BoardEdge::MinX => 0,
                BoardEdge::MaxX => i128::from(board.length.micrometres()),
            };
            let z = match installation.side.door_face {
                BoardFace::MinZ => 0,
                BoardFace::MaxZ => i128::from(board.thickness.micrometres()),
            };
            Some([
                world(pose, [x, 0, z])?,
                world(pose, [x, i128::from(board.width.micrometres()), z])?,
            ])
        })
    } else {
        None
    };
    // Unverified or out-of-bounds diagnostics may carry known numeric references,
    // but they must not be shown as valid cup/plate drilling locations.
    let valid = status.issues.is_empty();
    let cup = if valid && door_visible {
        door_pose
            .zip(status.references.as_ref())
            .and_then(|(pose, refs)| world(pose, refs.cup_center_um))
    } else {
        None
    };
    let plate = if valid && mount_visible {
        mount_pose
            .zip(status.references.as_ref())
            .and_then(|(pose, refs)| {
                Some([
                    world(pose, refs.plate_hole_centers_um[0])?,
                    world(pose, refs.plate_hole_centers_um[1])?,
                ])
            })
    } else {
        None
    };
    Guide {
        id: installation.id,
        axis,
        cup,
        plate,
        unavailable: status.references.is_none(),
        issues: status.issues,
    }
}

fn projected(camera: &Camera, rect: egui::Rect, point: [f64; 3]) -> Option<egui::Pos2> {
    camera.project(point, rect).filter(|p| rect.contains(*p))
}

fn warning_text(issues: &[InstallationIssue], pt: bool) -> &'static str {
    match issues.first() {
        Some(InstallationIssue::UnsupportedThickness) => {
            if pt {
                "Espessura inválida"
            } else {
                "Thickness unsupported"
            }
        }
        Some(InstallationIssue::UnsupportedOverlay) => {
            if pt {
                "K/R não confirmado"
            } else {
                "K/R unverified"
            }
        }
        Some(InstallationIssue::CupOutsideDoor) => {
            if pt {
                "Copo fora da porta"
            } else {
                "Cup outside door"
            }
        }
        Some(InstallationIssue::PlateOutsideMount) => {
            if pt {
                "Placa fora do suporte"
            } else {
                "Plate outside mount"
            }
        }
        Some(InstallationIssue::MissingPart(_)) => {
            if pt {
                "Peça ausente"
            } else {
                "Board missing"
            }
        }
        Some(InstallationIssue::MissingCatalog(_) | InstallationIssue::MissingVerifiedCatalog) => {
            if pt {
                "Fonte indisponível"
            } else {
                "Evidence unavailable"
            }
        }
        None => {
            if pt {
                "Relação exige revisão"
            } else {
                "Relationship needs review"
            }
        }
    }
}

fn dashed(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: egui::Color32) {
    let length = from.distance(to);
    if !length.is_finite() || length < 1.0 {
        return;
    }
    let vector = to - from;
    for index in 0..((length / 10.0).ceil() as usize).min(1000) {
        let start = index as f32 * 10.0 / length;
        let end = ((index as f32 * 10.0 + 6.0) / length).min(1.0);
        painter.line_segment(
            [from + vector * start, from + vector * end],
            egui::Stroke::new(1.5, color),
        );
    }
}

fn selected_group(project: &Project, selected: Uuid) -> Option<(Vec<Uuid>, bool)> {
    project
        .hinge_installations
        .iter()
        .find(|h| h.id == selected)?;
    let joint = project
        .door_joints
        .iter()
        .find(|joint| joint.hinge_installation_ids.contains(&selected));
    let mut ids = joint.map_or_else(
        || vec![selected],
        |joint| joint.hinge_installation_ids.clone(),
    );
    ids.sort_by_key(|id| *id != selected);
    Some((
        ids,
        joint.is_some_and(|joint| plan_my_cabinet::door_joint::needs_review(project, joint)),
    ))
}

/// Canvas-local projection and clipping. All shapes are painted without allocating a
/// response, so modal input isolation and scene picking retain their existing owner.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    ui: &egui::Ui,
    camera: &Camera,
    project: &Project,
    selection: &Selection,
    rect: egui::Rect,
    poses: Option<&HashMap<Uuid, Pose>>,
    selected: Option<Uuid>,
    pt: bool,
) {
    let Some(selected) = selected else { return };
    let Some((ids, relationship_warning)) = selected_group(project, selected) else {
        return;
    };
    if rect.width() < 180.0 || rect.height() < 120.0 {
        return;
    }
    let relationship = project
        .door_joints
        .iter()
        .any(|joint| joint.hinge_installation_ids.contains(&selected));
    let painter = ui.painter().with_clip_rect(rect);
    let passive = egui::Color32::from_rgb(39, 128, 145);
    let active = egui::Color32::from_rgb(178, 92, 23);
    let warning = egui::Color32::from_rgb(155, 56, 46);
    for (index, id) in ids.iter().enumerate() {
        let Some(hinge) = project.hinge_installations.iter().find(|h| h.id == *id) else {
            continue;
        };
        let guide = guide(project, selection, hinge, poses, relationship);
        let selected_guide = guide.id == selected;
        let invalid = !guide.issues.is_empty() || relationship_warning;
        let color = if invalid {
            warning
        } else if selected_guide {
            active
        } else {
            passive
        };
        let axis = guide
            .axis
            .and_then(|[a, b]| Some((camera.project(a, rect)?, camera.project(b, rect)?)));
        if let Some((a, b)) = axis {
            dashed(&painter, a, b, color);
        }
        let cup = guide.cup.and_then(|p| projected(camera, rect, p));
        let plate = guide
            .plate
            .and_then(|[a, b]| Some((projected(camera, rect, a)?, projected(camera, rect, b)?)));
        if let Some(cup) = cup {
            painter.circle_stroke(
                cup,
                if selected_guide { 7.0 } else { 5.0 },
                egui::Stroke::new(2.0, color),
            );
            painter.circle_filled(cup, 2.0, color);
        }
        if let Some((a, b)) = plate {
            for point in [a, b] {
                painter.line_segment(
                    [point + egui::vec2(-4.0, -4.0), point + egui::vec2(4.0, 4.0)],
                    egui::Stroke::new(1.5, color),
                );
                painter.line_segment(
                    [point + egui::vec2(-4.0, 4.0), point + egui::vec2(4.0, -4.0)],
                    egui::Stroke::new(1.5, color),
                );
            }
            dashed(&painter, a, b, color);
            if let Some(cup) = cup {
                dashed(&painter, cup, a + (b - a) * 0.5, color);
            }
        }
        // Match the tree's ordinal, rather than a UUID prefix (different UUIDs
        // can share that prefix). Keep callouts near their projected reference
        // and below the reserved camera/motion mode controls.
        let ordinal = project
            .hinge_installations
            .iter()
            .position(|hinge| hinge.id == guide.id)
            .unwrap_or(0)
            + 1;
        let title = if invalid {
            warning_text(&guide.issues, pt)
        } else if guide.unavailable {
            if pt {
                "Referência indisponível"
            } else {
                "Reference unavailable"
            }
        } else if pt {
            "Eixo · copo / placa"
        } else {
            "Axis · cup / plate"
        };
        let title = if rect.width() < 300.0 { "!" } else { title };
        let text = if invalid || guide.unavailable {
            format!("H{ordinal} · {title}")
        } else {
            format!("H{ordinal}")
        };
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), color);
        let size = galley.size() + egui::vec2(12.0, 8.0);
        let anchor = cup.or_else(|| axis.map(|(a, b)| a + (b - a) * 0.5));
        let desired = anchor
            .map(|point| point - egui::vec2(size.x + 16.0, size.y * 0.5))
            .unwrap_or(rect.left_top() + egui::vec2(8.0, 110.0 + index as f32 * 27.0));
        let location = egui::pos2(
            desired.x.clamp(
                rect.left() + 8.0,
                (rect.right() - size.x - 8.0).max(rect.left() + 8.0),
            ),
            desired.y.clamp(
                rect.top() + 104.0,
                (rect.bottom() - size.y - 30.0).max(rect.top() + 104.0),
            ),
        );
        if location.y + size.y > rect.bottom() - 30.0 {
            break;
        }
        let badge = egui::Rect::from_min_size(location, size).intersect(rect.shrink(2.0));
        painter.rect_filled(badge, 4.0, egui::Color32::from_rgb(255, 251, 244));
        painter.rect_stroke(
            badge,
            4.0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        painter.galley(location + egui::vec2(6.0, 4.0), galley, color);
        if selected_guide && let Some(anchor) = anchor {
            dashed(&painter, badge.right_center(), anchor, color);
        }
    }
    let note = if rect.width() < 300.0 {
        if pt {
            "Não é usinagem"
        } else {
            "Not machining"
        }
    } else if pt {
        "Referência visual · não é usinagem"
    } else {
        "Visual reference · not machining"
    };
    let note_at = rect.right_bottom() - egui::vec2(8.0, 8.0);
    painter.text(
        note_at,
        egui::Align2::RIGHT_BOTTOM,
        note,
        egui::FontId::proportional(11.0),
        warning,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::reference_fixture::{self, HINGE_IDS};
    use plan_my_cabinet::units::{Length, Quaternion};

    #[test]
    fn selected_identity_and_board_local_points_follow_only_the_moving_display_pose() {
        let project = reference_fixture::project();
        let hinge = project
            .hinge_installations
            .iter()
            .find(|h| h.id == HINGE_IDS[0])
            .unwrap();
        let other = project
            .hinge_installations
            .iter()
            .find(|h| h.id == HINGE_IDS[1])
            .unwrap();
        let selection = Selection::default();
        let closed = guide(&project, &selection, hinge, None, true);
        let other_guide = guide(&project, &selection, other, None, true);
        assert_eq!(closed.id, HINGE_IDS[0]);
        assert_ne!(closed.id, other_guide.id);
        assert_eq!(
            selected_group(&project, hinge.id).unwrap().0,
            HINGE_IDS[..2]
        );
        assert_eq!(
            selected_group(&project, HINGE_IDS[2]).unwrap().0,
            HINGE_IDS[2..]
        );
        assert_eq!(
            selected_group(&project, HINGE_IDS[1]).unwrap().0,
            [HINGE_IDS[1], HINGE_IDS[0]]
        );
        assert!(selected_group(&project, Uuid::new_v4()).is_none());
        assert!(closed.issues.is_empty());
        let door = project
            .boards
            .iter()
            .find(|b| b.id == hinge.door_board_id)
            .unwrap();
        let mount = project
            .boards
            .iter()
            .find(|b| b.id == hinge.mounting_board_id)
            .unwrap();
        let translated = Pose::new([420.0, 180.0, 30.0], Quaternion::IDENTITY).unwrap();
        let poses = HashMap::from([(door.id, translated)]);
        let moved = guide(&project, &selection, hinge, Some(&poses), true);
        let refs = hinge_installation::diagnose(&project, hinge)
            .references
            .unwrap();
        assert_eq!(moved.cup, world(translated, refs.cup_center_um));
        assert_ne!(closed.cup, moved.cup);
        assert_ne!(closed.axis, moved.axis);
        assert_eq!(closed.plate, moved.plate); // stationary mounting board
        let mount_pose = world_pose(&project, mount).unwrap();
        assert_eq!(
            moved.plate.unwrap()[0],
            world(mount_pose, refs.plate_hole_centers_um[0]).unwrap()
        );
        assert_eq!(
            project
                .boards
                .iter()
                .find(|b| b.id == door.id)
                .unwrap()
                .pose,
            door.pose
        );
    }

    #[test]
    fn invalid_references_are_withheld_and_hidden_boards_do_not_leave_markers() {
        let mut project = reference_fixture::project();
        let hinge = project
            .hinge_installations
            .iter()
            .find(|h| h.id == HINGE_IDS[0])
            .unwrap()
            .clone();
        let selection = Selection::default();
        let valid = guide(&project, &selection, &hinge, None, true);
        assert!(valid.cup.is_some() && valid.plate.is_some());
        project
            .boards
            .iter_mut()
            .find(|b| b.id == hinge.door_board_id)
            .unwrap()
            .thickness = Length::from_micrometres(1_000);
        let bad = guide(&project, &selection, &hinge, None, true);
        assert!(
            bad.issues
                .contains(&InstallationIssue::UnsupportedThickness)
        );
        assert!(bad.cup.is_none() && bad.plate.is_none());
        let mut hidden = Selection::default();
        hidden.hidden.insert(hinge.door_board_id);
        let hidden_guide = guide(&reference_fixture::project(), &hidden, &hinge, None, true);
        assert!(hidden_guide.cup.is_none() && hidden_guide.axis.is_none());
    }

    #[test]
    fn real_motion_poses_project_axis_and_cup_in_both_cameras_without_moving_plate() {
        let project = reference_fixture::project();
        let hinge = project
            .hinge_installations
            .iter()
            .find(|h| h.id == HINGE_IDS[0])
            .unwrap();
        let joint = project
            .door_joints
            .iter()
            .find(|j| j.hinge_installation_ids.contains(&hinge.id))
            .unwrap();
        let open_poses: HashMap<_, _> =
            plan_my_cabinet::door_joint::derived_poses(&project, joint, 45.0)
                .unwrap()
                .into_iter()
                .collect();
        let selection = Selection::default();
        let closed = guide(&project, &selection, hinge, None, true);
        let open = guide(&project, &selection, hinge, Some(&open_poses), true);
        assert_ne!(closed.cup, open.cup);
        assert_ne!(closed.axis, open.axis);
        assert_eq!(closed.plate, open.plate);
        for projection in [Projection::Orthographic, Projection::Perspective] {
            let camera = Camera {
                projection,
                ..Camera::reference_baseline()
            };
            let rect = egui::Rect::from_min_size(egui::pos2(150.0, 60.0), egui::vec2(800.0, 600.0));
            let cup = camera.project(open.cup.unwrap(), rect).unwrap();
            let axis = camera.project(open.axis.unwrap()[0], rect).unwrap();
            assert!(cup.is_finite() && axis.is_finite());
            assert_ne!(cup, camera.project(closed.cup.unwrap(), rect).unwrap());
        }
    }
}
