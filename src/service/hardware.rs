//! Hardware tools: hinge catalogs, hinge installations and door relationships.
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::catalog_pack::{self, CatalogRegistry};
use crate::domain::{BoardEdge, BoardFace, CatalogReference, HingeArm, HingeInstallation, Project};
use crate::hardware_catalog::{self, Trust};
use crate::hinge_installation::{
    self, InstallationStatus, edge_length, recommended_count, standard_positions,
};
use crate::service::dto::{Change, LengthInput, mm_f64};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult, installation_issue_text};
use crate::service::workspace::{Kind, Workspace, object_name};
use crate::units::Length;

fn edge_name(edge: BoardEdge) -> &'static str {
    match edge {
        BoardEdge::MinX => "-x",
        BoardEdge::MaxX => "+x",
        BoardEdge::MinY => "-y",
        BoardEdge::MaxY => "+y",
    }
}

fn face_name(face: BoardFace) -> &'static str {
    match face {
        BoardFace::MinZ => "-z",
        BoardFace::MaxZ => "+z",
    }
}

fn arm_name(arm: HingeArm) -> &'static str {
    match arm {
        HingeArm::FullOverlay => "full_overlay",
        HingeArm::HalfOverlay => "half_overlay",
        HingeArm::Inset => "inset",
    }
}

fn pairs(entry: &CatalogReference) -> Vec<Value> {
    hardware_catalog::facts(entry)
        .map(|f| {
            f.overlay_by_cup_edge
                .iter()
                .map(|p| json!({ "k_mm": mm_f64(p.cup_edge_setback), "r_or_f_mm": mm_f64(p.overlay) }))
                .collect()
        })
        .unwrap_or_default()
}

fn entry_json(project: &Project, entry: &CatalogReference) -> Value {
    let facts = hardware_catalog::facts(entry);
    json!({
        "catalog_id": entry.id,
        "name": entry.name,
        "product": entry.product_id,
        "plate": entry.plate_id,
        "arm": facts.map(|f| arm_name(f.arm)),
        "trust": match hardware_catalog::trust(entry) { Some(Trust::Reviewed) => "reviewed", Some(Trust::UserSupplied) => "user_supplied", None => "unverified" },
        "k_r_pairs": pairs(entry),
        "door_thickness_mm": facts.map(|f| [mm_f64(f.door_thickness_min), mm_f64(f.door_thickness_max)]),
        "opening_limit_degrees": facts.map(|f| f.opening_limit_degrees),
        "hinges_using_it": project.hinge_installations.iter().filter(|h| h.catalog_id == entry.id).count(),
    })
}

fn status_json(project: &Project, status: &InstallationStatus) -> Value {
    let installation = project
        .hinge_installations
        .iter()
        .find(|h| h.id == status.id);
    let um = |v: i128| (v as f64 / 1000.0 * 1000.0).round() / 1000.0;
    json!({
        "hinge_id": status.id,
        "door": installation.map(|h| object_name(project, h.door_board_id)),
        "mount": installation.map(|h| object_name(project, h.mounting_board_id)),
        "catalog": installation.and_then(|h| project.catalog.iter().find(|c| c.id == h.catalog_id)).map(|c| c.name.clone()),
        "side": installation.map(|h| json!({
            "door_edge": edge_name(h.side.door_edge),
            "door_face": face_name(h.side.door_face),
            "mount_front_edge": edge_name(h.side.mount_front_edge),
            "mount_face": face_name(h.side.mount_face),
        })),
        "door_y_mm": installation.map(|h| mm_f64(h.door_y)),
        "mount_y_mm": installation.map(|h| mm_f64(h.mount_y)),
        "k_mm": installation.map(|h| mm_f64(h.cup_edge_setback)),
        "r_or_f_mm": installation.map(|h| mm_f64(h.overlay)),
        "ok": status.issues.is_empty(),
        "issues": status.issues.iter().map(installation_issue_text).collect::<Vec<_>>(),
        "drilling": status.references.as_ref().map(|r| json!({
            "cup_center_on_door_mm": r.cup_center_um.map(|v| um(v) / 1000.0),
            "cup_face": face_name(r.cup_face),
            "cup_diameter_mm": mm_f64(r.cup_diameter),
            "cup_depth_mm": mm_f64(r.cup_depth),
            "plate_holes_on_mount_mm": r.plate_hole_centers_um.map(|p| p.map(|v| um(v) / 1000.0)),
            "plate_face": face_name(r.plate_face),
            "plate_front_offset_mm": mm_f64(r.plate_front_offset),
            "arm": arm_name(r.arm),
            "product": r.product_id,
            "source": r.source,
            "note": "Board-local coordinates in mm (x = length, y = width, z = thickness). Reference only; check against the manufacturer's sheet.",
        })),
    })
}

// ------------------------------------------------------------------- inputs

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct ListCatalogInput {
    /// Re-read user packs from disk.
    #[serde(default)]
    pub reload: bool,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct AddCatalogHingeInput {
    /// Pack id (see list_hinge_catalog). All omitted = the bundled reviewed
    /// full-overlay kit.
    #[serde(default)]
    pub pack: Option<String>,
    #[serde(default)]
    pub hinge: Option<String>,
    /// Variant code, e.g. "51MX153DRV00100".
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SuggestHingesInput {
    /// The door board.
    pub door: String,
    /// The side it hangs from; default: the nearest perpendicular board.
    #[serde(default)]
    pub mount: Option<String>,
    #[serde(default)]
    pub count: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct AddHingesInput {
    /// The door board.
    pub door: String,
    /// The side it hangs from; default: the nearest perpendicular board.
    #[serde(default)]
    pub mount: Option<String>,
    /// Pinned catalog entry; default the project's first verified one (the
    /// bundled kit is pinned when there is none).
    #[serde(default)]
    pub catalog: Option<String>,
    /// Default by door edge length: 2 up to 900 mm, 3 up to 1500, 4 up to 2000.
    #[serde(default)]
    pub count: Option<usize>,
    /// Distances along the hinge edge from its low end; default 100 mm from
    /// each end, evenly between.
    #[serde(default)]
    pub positions: Option<Vec<LengthInput>>,
    /// Cup edge setback K; default from the catalog table (the pair whose R
    /// matches the side thickness when there is one).
    #[serde(default)]
    pub k: Option<LengthInput>,
    /// Overlay R (or gap F for inset hinges); must be a catalog pair with K.
    #[serde(default)]
    pub r: Option<LengthInput>,
    /// Inset hinges: depth E (default the door thickness).
    #[serde(default)]
    pub e: Option<LengthInput>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct UpdateHingeInput {
    pub hinge: String,
    /// New position along the door edge.
    #[serde(default)]
    pub door_y: Option<LengthInput>,
    #[serde(default)]
    pub k: Option<LengthInput>,
    #[serde(default)]
    pub r: Option<LengthInput>,
    #[serde(default)]
    pub e: Option<LengthInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DoorRefInput {
    /// The door board (or door relationship).
    pub door: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct HingeRefInput {
    pub hinge: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct ListHingesInput {
    #[serde(default)]
    pub door: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateDoorInput {
    /// The door board or a door assembly (door + handle, …) that swings.
    pub moving: String,
    /// Default: the mount of the door's hinges.
    #[serde(default)]
    pub mount: Option<String>,
    /// Default: every hinge on that door and mount.
    #[serde(default)]
    pub hinges: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

impl Workspace {
    fn registry(&mut self, reload: bool) -> &CatalogRegistry {
        if reload {
            self.catalogs = CatalogRegistry::load(self.catalog_dir.as_deref());
        }
        &self.catalogs
    }

    pub fn list_hinge_catalog(&mut self, input: ListCatalogInput) -> ServiceResult<Value> {
        let dir = self.catalog_dir.as_ref().map(|d| d.display().to_string());
        let registry = self.registry(input.reload);
        let packs: Vec<_> = registry
            .packs
            .iter()
            .map(|loaded| {
                let pack = loaded.usable();
                json!({
                    "pack": pack.map(|p| p.id.clone()),
                    "manufacturer": pack.map(|p| p.manufacturer.clone()),
                    "version": pack.map(|p| p.version.clone()),
                    "errors": loaded.errors(),
                    "hinges": pack.map(|p| p.hinges.iter().map(|h| json!({
                        "hinge": h.id,
                        "name": h.name("en"),
                        "soft_close": h.soft_close,
                        "opening_degrees": h.opening_degrees,
                        "cup_mm": [mm_f64(h.cup_diameter), mm_f64(h.cup_depth)],
                        "door_thickness_mm": [mm_f64(h.door_thickness_min), mm_f64(h.door_thickness_max)],
                        "variants": h.variants.iter().map(|v| json!({
                            "variant": v.code,
                            "arm": arm_name(v.arm),
                            "plate": v.plate_code,
                            "k_r_pairs": v.k_table.iter().map(|(k, r)| json!([mm_f64(*k), mm_f64(*r)])).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>()),
                })
            })
            .collect();
        Ok(json!({ "user_pack_dir": dir, "packs": packs }))
    }

    pub fn list_project_catalog(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        Ok(
            json!({ "entries": project.catalog.iter().map(|c| entry_json(project, c)).collect::<Vec<_>>() }),
        )
    }

    pub fn add_catalog_hinge(
        &mut self,
        input: AddCatalogHingeInput,
    ) -> ServiceResult<Change<Value>> {
        let language = self.language.tag().to_owned();
        let reference = match (&input.pack, &input.hinge, &input.variant) {
            (None, None, None) => None,
            _ => {
                let registry = &self.catalogs;
                let mut found = None;
                for pack in registry.usable() {
                    if input.pack.as_ref().is_some_and(|p| *p != pack.id) {
                        continue;
                    }
                    for family in &pack.hinges {
                        if input.hinge.as_ref().is_some_and(|h| *h != family.id) {
                            continue;
                        }
                        for variant in &family.variants {
                            if input
                                .variant
                                .as_ref()
                                .is_some_and(|v| !v.eq_ignore_ascii_case(&variant.code))
                            {
                                continue;
                            }
                            if found.is_none() {
                                found =
                                    Some(catalog_pack::snapshot(pack, family, variant, &language));
                            }
                        }
                    }
                }
                Some(found.ok_or_else(|| {
                    ServiceError::not_found(
                        "catalog variant",
                        format!("{:?}/{:?}/{:?}", input.pack, input.hinge, input.variant),
                    )
                    .hint("See list_hinge_catalog.")
                })?)
            }
        };
        self.change(input.expected_revision, |editor| {
            let id = match reference {
                Some(reference) => hardware_catalog::add(editor, reference)?,
                None => hardware_catalog::add_builtin(editor)?,
            };
            let project = editor.project();
            let entry = project
                .catalog
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ServiceError::internal("entry vanished"))?;
            Ok((
                entry_json(project, entry),
                format!("Pinned '{}' to the project.", entry.name),
            ))
        })
    }

    /// Default catalog entry: the first verified pinned one.
    fn default_catalog(&self) -> ServiceResult<Option<Uuid>> {
        Ok(self
            .project()?
            .catalog
            .iter()
            .find(|c| hardware_catalog::is_verified(c))
            .map(|c| c.id))
    }

    pub fn suggest_hinges(&self, input: SuggestHingesInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let door = self.resolve(Kind::Board, &input.door)?;
        let mount = match &input.mount {
            Some(m) => Some(self.resolve(Kind::Board, m)?),
            None => hinge_installation::likely_mount(project, door),
        };
        let Some(mount) = mount else {
            return Ok(json!({
                "mount": null,
                "note": "No board stands next to this door at right angles; place the door against a side (describe_scene shows positions) or pass mount.",
            }));
        };
        let board = project
            .board(door)
            .ok_or_else(|| ServiceError::not_found("board", door))?;
        // Find the hinge edge from a probe fit.
        let probe = [board.length, board.width].iter().find_map(|half| {
            hinge_installation::fit(
                project,
                door,
                mount,
                Length::from_micrometres(half.micrometres() / 2),
            )
            .ok()
        });
        let Some((side, _)) = probe else {
            return Err(hinge_installation::fit(project, door, mount, Length::ZERO)
                .err()
                .map_or_else(
                    || {
                        ServiceError::new(
                            ErrorCode::InstallationError,
                            "the door cannot be hinged to that board",
                        )
                    },
                    ServiceError::from,
                ));
        };
        let edge = edge_length(side.door_edge, board.length, board.width);
        let count = input.count.unwrap_or_else(|| recommended_count(edge));
        let positions = standard_positions(edge, count);
        let fits: Vec<_> = positions
            .iter()
            .map(
                |y| match hinge_installation::fit(project, door, mount, *y) {
                    Ok((_, mount_y)) => {
                        json!({ "door_y_mm": mm_f64(*y), "mount_y_mm": mm_f64(mount_y) })
                    }
                    Err(e) => {
                        json!({ "door_y_mm": mm_f64(*y), "error": ServiceError::from(e).message })
                    }
                },
            )
            .collect();
        let catalog = self.default_catalog()?;
        let entry = catalog.and_then(|id| project.catalog.iter().find(|c| c.id == id));
        let mount_thickness = project.board(mount).map(|b| b.thickness);
        Ok(json!({
            "door": board.name,
            "mount": { "id": mount, "name": object_name(project, mount) },
            "hinge_edge": edge_name(side.door_edge),
            "cup_face": face_name(side.door_face),
            "edge_length_mm": mm_f64(edge),
            "recommended_count": recommended_count(edge),
            "positions": fits,
            "catalog": entry.map(|e| entry_json(project, e)),
            "default_pair": entry.and_then(|e| default_pair(e, mount_thickness)).map(|(k, r)| json!({ "k_mm": mm_f64(k), "r_or_f_mm": mm_f64(r) })),
            "note": if entry.is_none() { "No hinge is pinned yet; add_hinges pins the bundled kit automatically, or call add_catalog_hinge." } else { "" },
        }))
    }

    pub fn add_hinges(&mut self, input: AddHingesInput) -> ServiceResult<Change<Value>> {
        let door = self.resolve(Kind::Board, &input.door)?;
        let mount = match &input.mount {
            Some(m) => self.resolve(Kind::Board, m)?,
            None => hinge_installation::likely_mount(self.project()?, door).ok_or_else(|| {
                ServiceError::new(
                    ErrorCode::InstallationError,
                    "no board stands next to this door at right angles",
                )
                .hint("Place the door against its side, or pass mount.")
            })?,
        };
        let explicit_catalog = input
            .catalog
            .as_ref()
            .map(|c| self.resolve(Kind::Catalog, c))
            .transpose()?;
        let r = input.allow_rounding;
        let k = input
            .k
            .as_ref()
            .map(|v| v.non_negative("k", r))
            .transpose()?;
        let overlay = input
            .r
            .as_ref()
            .map(|v| v.non_negative("r", r))
            .transpose()?;
        let e = input
            .e
            .as_ref()
            .map(|v| v.non_negative("e", r))
            .transpose()?;
        let positions = input
            .positions
            .as_ref()
            .map(|list| {
                list.iter()
                    .map(|p| p.non_negative("positions", r))
                    .collect::<ServiceResult<Vec<_>>>()
            })
            .transpose()?;
        let needs_builtin = explicit_catalog.is_none() && self.default_catalog()?.is_none();
        let dry_run = input.dry_run;
        let count_wanted = input.count;
        let run = |editor: &mut crate::commands::ProjectEditor,
                   commit: bool|
         -> ServiceResult<(Value, String)> {
            let catalog = match explicit_catalog {
                Some(id) => id,
                None => match editor
                    .project()
                    .catalog
                    .iter()
                    .find(|c| hardware_catalog::is_verified(c))
                {
                    Some(c) => c.id,
                    None if commit && needs_builtin => hardware_catalog::add_builtin(editor)?,
                    None => {
                        return Err(ServiceError::new(
                            ErrorCode::CatalogError,
                            "no verified hinge is pinned",
                        )
                        .hint("Call add_catalog_hinge (dry runs do not pin one)."));
                    }
                },
            };
            let project = editor.project();
            let board = project
                .board(door)
                .ok_or_else(|| ServiceError::not_found("board", door))?
                .clone();
            let entry = project
                .catalog
                .iter()
                .find(|c| c.id == catalog)
                .ok_or_else(|| ServiceError::not_found("catalog entry", catalog))?;
            let facts = hardware_catalog::facts(entry).ok_or_else(|| {
                ServiceError::new(ErrorCode::CatalogError, "the catalog entry is not verified")
            })?;
            let inset = facts.arm.is_inset();
            let mount_thickness = project.board(mount).map(|b| b.thickness);
            let (k, overlay) = match (k, overlay) {
                (Some(k), Some(o)) => (k, o),
                _ => default_pair(entry, mount_thickness).ok_or_else(|| {
                    ServiceError::new(ErrorCode::CatalogError, "the catalog has no K/R pairs")
                })?,
            };
            let probe = [board.length, board.width]
                .iter()
                .find_map(|half| {
                    hinge_installation::fit(
                        project,
                        door,
                        mount,
                        Length::from_micrometres(half.micrometres() / 2),
                    )
                    .ok()
                })
                .ok_or_else(|| {
                    hinge_installation::fit(project, door, mount, Length::ZERO)
                        .err()
                        .map_or_else(
                            || ServiceError::new(ErrorCode::InstallationError, "cannot fit"),
                            ServiceError::from,
                        )
                })?;
            let edge = edge_length(probe.0.door_edge, board.length, board.width);
            let positions = positions.clone().unwrap_or_else(|| {
                standard_positions(
                    edge,
                    count_wanted.unwrap_or_else(|| recommended_count(edge)),
                )
            });
            let mut statuses = Vec::new();
            for door_y in positions {
                let project = editor.project();
                let (side, mount_y) = hinge_installation::fit(project, door, mount, door_y)?;
                let installation = HingeInstallation {
                    id: Uuid::new_v4(),
                    door_board_id: door,
                    mounting_board_id: mount,
                    catalog_id: catalog,
                    side,
                    door_y,
                    mount_y,
                    cup_edge_setback: k,
                    overlay,
                    inset_depth: if inset {
                        e.unwrap_or(board.thickness)
                    } else {
                        e.unwrap_or(Length::ZERO)
                    },
                };
                let status = if commit {
                    hinge_installation::create(editor, installation)?
                } else {
                    hinge_installation::preview(project, &installation)?
                };
                statuses.push(status);
            }
            let project = editor.project();
            let rows: Vec<_> = statuses.iter().map(|s| status_json(project, s)).collect();
            let problems = statuses.iter().filter(|s| !s.issues.is_empty()).count();
            Ok((
                json!({ "hinges": rows, "mount": object_name(project, mount), "k_mm": mm_f64(k), "r_or_f_mm": mm_f64(overlay) }),
                format!(
                    "{} {} hinge(s) on '{}' hung from '{}'{}.",
                    if commit { "Added" } else { "Would add" },
                    statuses.len(),
                    board.name,
                    object_name(project, mount),
                    if problems > 0 {
                        format!(" — {problems} with issues")
                    } else {
                        String::new()
                    }
                ),
            ))
        };
        if dry_run {
            let mut scratch = crate::commands::ProjectEditor::new(self.project()?.clone())?;
            let (result, summary) = run(&mut scratch, false)?;
            return self.preview_only(result, summary);
        }
        let mut change = self.change(input.expected_revision, |editor| run(editor, true))?;
        if needs_builtin {
            change.warnings.push(
                "No hinge was pinned, so the bundled reviewed kit was added to the project.".into(),
            );
        }
        Ok(change)
    }

    pub fn update_hinge(&mut self, input: UpdateHingeInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Hinge, &input.hinge)?;
        let r = input.allow_rounding;
        let door_y = input
            .door_y
            .as_ref()
            .map(|v| v.non_negative("door_y", r))
            .transpose()?;
        let k = input
            .k
            .as_ref()
            .map(|v| v.non_negative("k", r))
            .transpose()?;
        let overlay = input
            .r
            .as_ref()
            .map(|v| v.non_negative("r", r))
            .transpose()?;
        let e = input
            .e
            .as_ref()
            .map(|v| v.non_negative("e", r))
            .transpose()?;
        self.change(input.expected_revision, |editor| {
            let mut status = None;
            if let Some(y) = door_y {
                status = Some(hinge_installation::move_to(editor, id, y)?);
            }
            if k.is_some() || overlay.is_some() || e.is_some() {
                let mut next = editor
                    .project()
                    .hinge_installations
                    .iter()
                    .find(|h| h.id == id)
                    .cloned()
                    .ok_or_else(|| ServiceError::not_found("hinge", id))?;
                if let Some(k) = k {
                    next.cup_edge_setback = k;
                }
                if let Some(o) = overlay {
                    next.overlay = o;
                }
                if let Some(e) = e {
                    next.inset_depth = e;
                }
                status = Some(hinge_installation::update(editor, next)?);
            }
            let project = editor.project();
            let status = match status {
                Some(s) => s,
                None => {
                    let h = project
                        .hinge_installations
                        .iter()
                        .find(|h| h.id == id)
                        .ok_or_else(|| ServiceError::not_found("hinge", id))?;
                    hinge_installation::diagnose(project, h)
                }
            };
            Ok((status_json(project, &status), "Updated the hinge.".into()))
        })
    }

    pub fn space_hinges_evenly(&mut self, input: DoorRefInput) -> ServiceResult<Change<Value>> {
        let door = self.resolve(Kind::Board, &input.door)?;
        self.change(input.expected_revision, |editor| {
            hinge_installation::space_evenly(editor, door)?;
            let project = editor.project();
            let rows: Vec<_> = project
                .hinge_installations
                .iter()
                .filter(|h| h.door_board_id == door)
                .map(|h| json!({ "hinge_id": h.id, "door_y_mm": mm_f64(h.door_y) }))
                .collect();
            Ok((
                json!({ "hinges": rows }),
                format!("Spaced the hinges of '{}'.", object_name(project, door)),
            ))
        })
    }

    pub fn remove_hinge(&mut self, input: HingeRefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Hinge, &input.hinge)?;
        self.change(input.expected_revision, |editor| {
            let doors_before = editor.project().door_joints.len();
            hinge_installation::remove(editor, id)?;
            let removed = doors_before - editor.project().door_joints.len();
            Ok((
                json!({ "doors_removed": removed }),
                "Removed the hinge.".into(),
            ))
        })
    }

    pub fn list_hinges(&self, input: ListHingesInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let door = input
            .door
            .as_ref()
            .map(|d| self.resolve(Kind::Board, d))
            .transpose()?;
        let rows: Vec<_> = hinge_installation::diagnose_all(project)
            .iter()
            .filter(|s| {
                door.is_none_or(|d| {
                    project
                        .hinge_installations
                        .iter()
                        .any(|h| h.id == s.id && h.door_board_id == d)
                })
            })
            .map(|s| status_json(project, s))
            .collect();
        Ok(json!({ "revision": project.revision, "hinges": rows }))
    }

    pub fn create_door(&mut self, input: CreateDoorInput) -> ServiceResult<Change<Value>> {
        let project = self.project()?;
        let root = self.resolve(Kind::Object, &input.moving)?;
        // Boards of the moving part.
        let door_boards: Vec<Uuid> = if project.board(root).is_some() {
            vec![root]
        } else {
            let mut scope = std::collections::HashSet::from([root]);
            loop {
                let before = scope.len();
                for a in &project.assemblies {
                    if a.parent_id.is_some_and(|p| scope.contains(&p)) {
                        scope.insert(a.id);
                    }
                }
                if scope.len() == before {
                    break;
                }
            }
            project
                .boards
                .iter()
                .filter(|b| b.parent_id.is_some_and(|p| scope.contains(&p)))
                .map(|b| b.id)
                .collect()
        };
        let hinges: Vec<Uuid> = if input.hinges.is_empty() {
            project
                .hinge_installations
                .iter()
                .filter(|h| door_boards.contains(&h.door_board_id))
                .map(|h| h.id)
                .collect()
        } else {
            self.resolve_all(Kind::Hinge, &input.hinges)?
        };
        if hinges.is_empty() {
            return Err(
                ServiceError::new(ErrorCode::JointError, "the door has no hinges")
                    .hint("Call add_hinges first."),
            );
        }
        let mount = match &input.mount {
            Some(m) => self.resolve(Kind::Board, m)?,
            None => project
                .hinge_installations
                .iter()
                .find(|h| h.id == hinges[0])
                .map(|h| h.mounting_board_id)
                .ok_or_else(|| ServiceError::not_found("hinge", hinges[0]))?,
        };
        let id = Uuid::new_v4();
        let preview = crate::door_joint::preview(project, id, root, mount, hinges.clone())?;
        let describe = |project: &Project, preview: &crate::door_joint::JointPreview| {
            json!({
                "door_id": preview.joint.id,
                "moving": preview.moving_members.iter().map(|m| object_name(project, *m)).collect::<Vec<_>>(),
                "mount": object_name(project, mount),
                "hinges": hinges.len(),
                "axis_origin_mm": preview.joint.axis_origin_mm.map(|v| (v * 1000.0).round() / 1000.0),
                "axis_direction": preview.joint.axis_direction.map(|v| (v * 1e6).round() / 1e6),
                "hinge_issues": preview.installation_statuses.iter().flat_map(|s| s.issues.iter().map(installation_issue_text)).collect::<Vec<_>>(),
            })
        };
        let preview_json = describe(project, &preview);
        if input.dry_run {
            return self.preview_only(preview_json, "Dry run: nothing changed.".into());
        }
        self.change(input.expected_revision, |editor| {
            crate::door_joint::confirm(editor, preview)?;
            let project = editor.project();
            let joint = project
                .door_joints
                .iter()
                .find(|j| j.id == id)
                .ok_or_else(|| ServiceError::internal("door vanished"))?;
            let mut out = preview_json;
            out["opening_limit_degrees"] =
                json!(crate::door_joint::opening_limit(project, joint).ok());
            Ok((
                out,
                format!("'{}' now opens on its hinges.", object_name(project, root)),
            ))
        })
    }

    pub fn remove_door(&mut self, input: DoorRefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Door, &input.door)?;
        self.change(input.expected_revision, |editor| {
            crate::door_joint::remove(editor, id)?;
            Ok((
                json!({}),
                "Removed the door relationship (hinges stay).".into(),
            ))
        })
    }

    pub fn list_doors(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let rows: Vec<_> = project
            .door_joints
            .iter()
            .map(|j| {
                json!({
                    "door_id": j.id,
                    "moving": object_name(project, j.moving_root_id),
                    "mount": object_name(project, j.mounting_board_id),
                    "hinges": j.hinge_installation_ids.len(),
                    "needs_review": crate::door_joint::needs_review(project, j),
                    "opening_limit_degrees": crate::door_joint::opening_limit(project, j).ok(),
                })
            })
            .collect();
        Ok(json!({ "revision": project.revision, "doors": rows }))
    }
}

/// The K/R pair whose R equals the side thickness (a full overlay that covers
/// the side edge), else the first pair in the table.
fn default_pair(
    entry: &CatalogReference,
    mount_thickness: Option<Length>,
) -> Option<(Length, Length)> {
    let facts = hardware_catalog::facts(entry)?;
    let table = &facts.overlay_by_cup_edge;
    mount_thickness
        .and_then(|t| table.iter().find(|p| p.overlay == t))
        .or_else(|| table.first())
        .map(|p| (p.cup_edge_setback, p.overlay))
}
