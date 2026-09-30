//! "Add then edit": each Add creates the hardware right away from the current
//! selection, with sensible defaults, and opens its inspector. A picker (the
//! old dialog) appears only when something must be chosen first.
use crate::hardware_ui::{FootModel, foot_models, resolve_foot};
use crate::*;
use plan_my_cabinet::assembly_edit::{default_foot_world, default_placeholder_world};
use plan_my_cabinet::domain::{CatalogReference, HardwareKind, HingeInstallation};
use plan_my_cabinet::{door_joint, hardware_catalog, hinge_installation, slide_installation};

impl DesktopApp {
    /// The assembly the selection belongs to: the selected assembly, or the
    /// selected board's or hardware item's parent.
    pub(crate) fn selection_parent(&self) -> Option<Uuid> {
        let project = self.editor.project();
        let id = self.selection.active?;
        if project.assemblies.iter().any(|a| a.id == id) {
            return Some(id);
        }
        project
            .board(id)
            .and_then(|b| b.parent_id)
            .or_else(|| project.hardware.iter().find(|h| h.id == id)?.parent_id)
    }

    /// The top-level assembly of the selection (the cabinet a foot goes under).
    fn selection_cabinet(&self) -> Option<Uuid> {
        let project = self.editor.project();
        let mut current = self.selection_parent()?;
        while let Some(parent) = project
            .assemblies
            .iter()
            .find(|a| a.id == current)
            .and_then(|a| a.parent_id)
        {
            current = parent;
        }
        Some(current)
    }

    fn inspect_new(&mut self, target: InspectorTarget) {
        self.request_inspect(target);
    }

    /// Returns false when a picker is needed instead.
    pub(crate) fn add_foot_now(&mut self) -> bool {
        let project = self.editor.project();
        // The model used last, else the first pinned foot, else the catalog's first.
        let last = project.hardware.iter().rev().find_map(|h| match h.kind {
            HardwareKind::Catalog { catalog_id } if project.foot_spec(h).is_some() => {
                Some(FootModel::Pinned(catalog_id))
            }
            _ => None,
        });
        let Some(model) = last.or_else(|| foot_models(self).into_iter().next().map(|(m, _)| m))
        else {
            return false;
        };
        let Some((catalog_id, pin)) = resolve_foot(self, &model) else {
            return false;
        };
        let size = pin
            .as_ref()
            .and_then(CatalogReference::foot)
            .or_else(|| {
                project
                    .catalog
                    .iter()
                    .find(|c| c.id == catalog_id)
                    .and_then(CatalogReference::foot)
            })
            .map(|s| s.local_size().map(|v| v.micrometres() as f64 / 1000.0))
            .unwrap_or([50.0; 3]);
        let parent = self.selection_cabinet();
        let world = default_foot_world(project, parent, size);
        let name = self.localizer.text("foot-default-name");
        match self
            .editor
            .create_foot(name, catalog_id, pin, parent, world)
        {
            Ok(id) => {
                self.inspect_new(InspectorTarget::Hardware(id));
                true
            }
            Err(_) => false,
        }
    }

    pub(crate) fn add_placeholder_now(&mut self) -> bool {
        let parent = self.selection_parent();
        let world = default_placeholder_world(self.editor.project(), parent);
        let size = Length::from_micrometres(100_000);
        let name = self.localizer.text("hardware-default-name");
        match self
            .editor
            .create_placeholder(name, [size; 3], parent, world)
        {
            Ok(id) => {
                self.inspect_new(InspectorTarget::Hardware(id));
                true
            }
            Err(_) => false,
        }
    }

    /// Slides on the selected drawer. An existing pair is inspected instead.
    pub(crate) fn add_slides_now(&mut self) -> bool {
        let project = self.editor.project();
        let Some(drawer) = self
            .selection
            .active
            .and_then(|id| slide_installation::drawer_root(project, id))
        else {
            return false;
        };
        if let Some(existing) = slide_installation::for_drawer(project, drawer) {
            let id = existing.id;
            self.inspect_new(InspectorTarget::Slide(id));
            return true;
        }
        // The family used last in this project, else the default.
        let key = project
            .catalog
            .iter()
            .rev()
            .filter(|c| c.slide().is_some())
            .find_map(|c| c.origin.as_ref())
            .map(|o| slide_ui::FamilyKey {
                pack: o.pack_id.clone(),
                family: o.item_id.clone(),
            })
            .unwrap_or_else(|| {
                let (pack, family) = hardware_catalog::DEFAULT_SLIDE_FAMILY;
                slide_ui::FamilyKey {
                    pack: pack.into(),
                    family: family.into(),
                }
            });
        let lengths = slide_ui::lengths(self, &key);
        let Ok(proposal) = slide_installation::propose(project, drawer, &lengths) else {
            return false;
        };
        if !proposal.status.issues.is_empty() {
            return false;
        }
        let pinned = project.catalog.iter().find(|c| {
            c.product_id == proposal.catalog.product_id
                && c.item == proposal.catalog.item
                && c.origin == proposal.catalog.origin
        });
        let installation = plan_my_cabinet::domain::SlideInstallation {
            id: Uuid::new_v4(),
            catalog_id: pinned.map_or(proposal.catalog.id, |c| c.id),
            drawer_root_id: proposal.detected.drawer_root,
            drawer_sides: proposal.detected.drawer_sides,
            cabinet_sides: proposal.detected.cabinet_sides,
            sides: proposal.detected.sides(),
            height: proposal.height,
            setback: proposal.setback,
        };
        let id = installation.id;
        let result = if pinned.is_some() {
            slide_installation::create(&mut self.editor, installation)
        } else {
            slide_installation::create_with_catalog(
                &mut self.editor,
                proposal.catalog,
                installation,
            )
        };
        if result.is_ok() {
            self.inspect_new(InspectorTarget::Slide(id));
        }
        result.is_ok()
    }

    /// The pinned verified hinge to use: the one used last, else the first.
    fn default_hinge_catalog(&self) -> Option<Uuid> {
        let project = self.editor.project();
        project
            .hinge_installations
            .last()
            .map(|h| h.catalog_id)
            .filter(|id| {
                project
                    .catalog
                    .iter()
                    .any(|c| c.id == *id && hardware_catalog::is_verified(c))
            })
            .or_else(|| {
                project
                    .catalog
                    .iter()
                    .find(|c| hardware_catalog::is_verified(c))
                    .map(|c| c.id)
            })
    }

    /// One more hinge on a door board, at a free standard position.
    pub(crate) fn next_hinge(&self, door: Uuid) -> Option<HingeInstallation> {
        let project = self.editor.project();
        let catalog = self.default_hinge_catalog()?;
        let existing: Vec<_> = project
            .hinge_installations
            .iter()
            .filter(|h| h.door_board_id == door)
            .collect();
        let mount = existing
            .first()
            .map(|h| h.mounting_board_id)
            .or_else(|| hinge_installation::likely_mount(project, door))?;
        let mut template = hinge_installation::standard_set(project, door, mount, catalog, Some(2))
            .ok()?
            .into_iter()
            .next()?;
        let board = project.board(door)?;
        let edge =
            hinge_installation::edge_length(template.side.door_edge, board.length, board.width);
        let taken: Vec<_> = existing.iter().map(|h| h.door_y).collect();
        let door_y = hinge_installation::next_position(edge, &taken);
        let (side, mount_y) = hinge_installation::fit(project, door, mount, door_y).ok()?;
        template.side = side;
        template.door_y = door_y;
        template.mount_y = mount_y;
        Some(template)
    }

    /// A hinge on the selected board (added to its door when it has one).
    pub(crate) fn add_hinge_now(&mut self) -> bool {
        let project = self.editor.project();
        let Some(door) = self
            .selection
            .active
            .filter(|id| project.board(*id).is_some())
        else {
            return false;
        };
        let Some(hinge) = self.next_hinge(door) else {
            return false;
        };
        let id = hinge.id;
        let joint = project
            .door_joints
            .iter()
            .find(|j| door_joint::moving_members(project, j.moving_root_id).contains(&door))
            .map(|j| j.id);
        let ok = match joint {
            Some(joint) => door_joint::add_hinge(&mut self.editor, joint, hinge).is_ok(),
            None => hinge_installation::create(&mut self.editor, hinge).is_ok(),
        };
        if ok {
            self.inspect_new(InspectorTarget::Installation(id));
        }
        ok
    }

    /// A door relationship on the selected board or assembly: its loose
    /// hinges, or standard hinges when it has none.
    pub(crate) fn add_door_now(&mut self) -> bool {
        let project = self.editor.project();
        let Some(root) = self.selection.active.filter(|id| {
            project.board(*id).is_some() || project.assemblies.iter().any(|a| a.id == *id)
        }) else {
            return false;
        };
        let members = door_joint::moving_members(project, root);
        if project
            .door_joints
            .iter()
            .any(|j| members.contains(&j.moving_root_id) || j.moving_root_id == root)
        {
            return false;
        }
        let loose: Vec<_> = project
            .hinge_installations
            .iter()
            .filter(|h| members.contains(&h.door_board_id))
            .collect();
        let id = Uuid::new_v4();
        let result = if let Some(first) = loose.first() {
            let mount = first.mounting_board_id;
            let ids = loose
                .iter()
                .filter(|h| h.mounting_board_id == mount)
                .map(|h| h.id)
                .collect();
            door_joint::create_with_hinges(&mut self.editor, id, root, mount, Vec::new(), ids)
        } else {
            let door = if project.board(root).is_some() {
                Some(root)
            } else {
                // The largest board of the door assembly.
                project
                    .boards
                    .iter()
                    .filter(|b| members.contains(&b.id))
                    .max_by_key(|b| {
                        i128::from(b.length.micrometres()) * i128::from(b.width.micrometres())
                    })
                    .map(|b| b.id)
            };
            let Some(door) = door else {
                return false;
            };
            let (Some(catalog), Some(mount)) = (
                self.default_hinge_catalog(),
                hinge_installation::likely_mount(project, door),
            ) else {
                return false;
            };
            let Ok(hinges) = hinge_installation::standard_set(project, door, mount, catalog, None)
            else {
                return false;
            };
            door_joint::create_with_hinges(&mut self.editor, id, root, mount, hinges, Vec::new())
        };
        if result.is_ok() {
            self.inspect_new(InspectorTarget::Door(id));
        }
        result.is_ok()
    }
}
