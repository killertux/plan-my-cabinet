//! Welcome template setup presentation. The active project is never passed to
//! this module: `Accept` is a request for the host to resolve replacement
//! guards, then call `TemplateSetup::generate` on the staged snapshot.
use std::collections::BTreeMap;

use eframe::egui::{self, Color32, RichText};
use plan_my_cabinet::dimension_input::{Locale, format_length, parse_length};
use plan_my_cabinet::domain::{BoardGrain, SrgbColor};
use plan_my_cabinet::first_fit::FirstFit;
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::template_recipes::{RecipeError, RecipeErrorKind};
use plan_my_cabinet::template_setup::{
    MaterialRole, ProposedLength, SetupError, TemplateField, TemplateKind, TemplateReview,
    TemplateSetup,
};
use plan_my_cabinet::units::{Conversion, Length, Unit};

use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};

const MUTED: Color32 = Color32::from_rgb(110, 101, 90);
const WARN: Color32 = Color32::from_rgb(138, 90, 18);
const DANGER: Color32 = Color32::from_rgb(180, 65, 47);
const BORDER: Color32 = Color32::from_rgb(221, 215, 205);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Project,
    Materials,
    Dimensions,
    Review,
}

impl Stage {
    fn next(self) -> Self {
        match self {
            Self::Project => Self::Materials,
            Self::Materials => Self::Dimensions,
            Self::Dimensions | Self::Review => Self::Review,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Project | Self::Materials => Self::Project,
            Self::Dimensions => Self::Materials,
            Self::Review => Self::Dimensions,
        }
    }
}

/// Accept does not create or replace a project. Keep this controller alive
/// while save/picker/navigation guards run; call `close` only on host success.
#[derive(Clone, Debug)]
pub enum TemplateSetupIntent {
    Accept(TemplateSetup),
    Cancel,
}

#[derive(Clone, Debug)]
struct LengthEntry {
    text: String,
    /// Captured when this field is created; later unit changes do not
    /// reinterpret pending unsuffixed input.
    entry_unit: Unit,
    invalid: bool,
}

impl LengthEntry {
    fn new_mm(value: i64, entry_unit: Unit) -> Self {
        Self {
            text: format!("{value} mm"),
            entry_unit,
            invalid: false,
        }
    }

    fn parse(&mut self) -> Option<ProposedLength> {
        let value = parse_length(&self.text, self.entry_unit)
            .ok()
            .map(|p| p.conversion);
        self.invalid = value.is_none();
        value.map(ProposedLength::new)
    }
}

#[derive(Clone, Debug)]
struct MaterialEntry {
    name: String,
    thickness: LengthEntry,
    proposal: Option<ProposedLength>,
    grain: BoardGrain,
    color: Option<SrgbColor>,
}

impl MaterialEntry {
    fn new(unit: Unit) -> Self {
        let mut thickness = LengthEntry::new_mm(18, unit);
        let proposal = thickness.parse();
        Self {
            name: String::new(),
            thickness,
            proposal,
            grain: BoardGrain::Unrestricted,
            color: None,
        }
    }

    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 256
            && self.proposal.is_some_and(|p| {
                p.conversion.suggested().micrometres() > 0
                    && (!matches!(p.conversion, Conversion::NeedsConfirmation(_))
                        || p.rounding_confirmed)
            })
    }
}

/// All widget text and nested material fields live here across frames.
/// `TemplateSetup` is independent of any open document, including its stock.
pub struct TemplateSetupUi {
    pub setup: TemplateSetup,
    pub stage: Stage,
    fields: BTreeMap<TemplateField, LengthEntry>,
    count_text: String,
    material: Option<MaterialEntry>,
    parent: ModalChrome,
    child: ModalChrome,
    restore_material_button: bool,
}

impl TemplateSetupUi {
    pub fn new(
        kind: TemplateKind,
        name: impl Into<String>,
        currency: Currency,
        unit: Unit,
    ) -> Self {
        let mut setup = TemplateSetup::new(kind, name, currency, unit);
        let mut fields = BTreeMap::new();
        for &(field, mm) in &[
            (TemplateField::Width, 600),
            (TemplateField::Depth, 560),
            (TemplateField::Height, 720),
            (TemplateField::RailWidth, 80),
            (TemplateField::ShelfHeight, 320),
            (TemplateField::BoxDepth, 500),
            (TemplateField::SideClearance, 13),
            (TemplateField::RearClearance, 20),
            (TemplateField::VerticalClearance, 8),
            (TemplateField::FrontReveal, 3),
            (TemplateField::FrontGap, 3),
        ] {
            if kind.fields().contains(&field) {
                let mut entry = LengthEntry::new_mm(mm, unit);
                setup
                    .dimensions
                    .insert(field, entry.parse().expect("built-in defaults are exact millimetres"));
                fields.insert(field, entry);
            }
        }
        if kind == TemplateKind::Drawers {
            setup.drawer_count = Some(3);
        }
        Self {
            setup,
            stage: Stage::Project,
            fields,
            count_text: "3".into(),
            material: None,
            parent: ModalChrome::new(egui::Id::new("template-setup"))
                .width(620.0)
                .first_focus(egui::Id::new("template-project-name")),
            child: ModalChrome::new(egui::Id::new("template-setup-material"))
                .width(480.0)
                .first_focus(egui::Id::new("template-material-name")),
            restore_material_button: false,
        }
    }

    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.parent.is_active() || self.child.is_active()
    }

    /// Called only after host generation succeeds. Failure/Stay keeps every
    /// field, role assignment, consent and review available for retry.
    pub fn close(&mut self, ctx: &egui::Context) {
        self.child.close(ctx);
        self.parent.close(ctx);
    }

    fn review(&self) -> Result<TemplateReview, Vec<SetupError>> {
        self.setup.review()
    }

    pub fn show(&mut self, ctx: &egui::Context, l: &Localizer) -> Option<TemplateSetupIntent> {
        let mut go_back = false;
        let mut open_material = false;
        let mut material_button_id = None;
        let stage = self.stage;
        let kind = self.setup.kind;
        let review = if stage == Stage::Review {
            Some(self.review())
        } else {
            None
        };
        let valid = match stage {
            Stage::Project => valid_name(&self.setup.project_name),
            Stage::Materials => {
                !self.setup.materials.is_empty()
                    && kind.roles().iter().all(|role| {
                        self.setup
                            .roles
                            .get(role)
                            .is_some_and(|id| self.setup.materials.iter().any(|m| m.id == *id))
                    })
            }
            Stage::Dimensions => {
                self.fields.values().all(|f| !f.invalid)
                    && (kind != TemplateKind::Drawers || self.setup.drawer_count.is_some())
            }
            Stage::Review => review.as_ref().is_some_and(Result::is_ok),
        };
        let title = format!(
            "{} · {}",
            l.text("template-setup-title"),
            l.text(kind_key(kind))
        );
        let action = self
            .parent
            .show(
                ctx,
                &title,
                ModalActions {
                    cancel: &l.text("template-setup-cancel"),
                    confirm: &l.text(if stage == Stage::Review {
                        "template-setup-generate"
                    } else {
                        "template-setup-next"
                    }),
                },
                |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.label(RichText::new(l.text(stage_key(stage))).strong());
                    ui.label(
                        RichText::new(l.text("template-setup-staged"))
                            .small()
                            .color(MUTED),
                    );
                    if stage != Stage::Project && ui.link(l.text("template-setup-back")).clicked() {
                        go_back = true;
                    }
                    ui.add_space(12.0);
                    match stage {
                        Stage::Project => project_fields(ui, l, &mut self.setup),
                        Stage::Materials => {
                            material_roles(
                                ui,
                                l,
                                &mut self.setup,
                                &mut open_material,
                                &mut material_button_id,
                            );
                        }
                        Stage::Dimensions => {
                            dimensions(
                                ui,
                                l,
                                &mut self.setup,
                                &mut self.fields,
                                &mut self.count_text,
                            );
                        }
                        Stage::Review => match review.as_ref().expect("review stage") {
                            Ok(review) => review_panel(ui, l, &self.setup, review),
                            Err(errors) => errors_panel(ui, l, &self.setup, errors),
                        },
                    }
                    ((), valid && !go_back && !open_material)
                },
            )
            .action;
        if self.restore_material_button {
            if let Some(id) = material_button_id {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            self.restore_material_button = false;
        }
        if go_back {
            self.stage = stage.previous();
            self.parent.body_replaced(ctx);
        }
        if open_material {
            self.material = Some(MaterialEntry::new(self.setup.input_unit));
        }
        if self.material.is_some() {
            self.show_material(ctx, l);
            return None; // the child owns keys this frame, including its opening frame
        }
        if go_back || open_material {
            return None;
        }
        match action {
            ModalAction::Cancel => {
                self.close(ctx);
                Some(TemplateSetupIntent::Cancel)
            }
            ModalAction::Confirm if valid && stage == Stage::Review => {
                Some(TemplateSetupIntent::Accept(self.setup.clone()))
            }
            ModalAction::Confirm if valid => {
                self.stage = stage.next();
                self.parent.body_replaced(ctx);
                None
            }
            _ => None,
        }
    }

    fn show_material(&mut self, ctx: &egui::Context, l: &Localizer) {
        let Some(material) = self.material.as_mut() else {
            return;
        };
        let result = self.child.show(
            ctx,
            &l.text("template-setup-material-title"),
            ModalActions {
                cancel: &l.text("template-setup-material-cancel"),
                confirm: &l.text("template-setup-material-add"),
            },
            |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.label(l.text("template-setup-material-name"));
                ui.add(
                    egui::TextEdit::singleline(&mut material.name)
                        .id(egui::Id::new("template-material-name")),
                );
                if !valid_name(&material.name) {
                    ui.colored_label(DANGER, l.text("template-setup-error-name"));
                }
                length_field(
                    ui,
                    l,
                    "template-setup-material-thickness",
                    &mut material.thickness,
                    &mut material.proposal,
                );
                grain_field(ui, l, &mut material.grain);
                ui.label(l.text("template-setup-color"));
                ui.horizontal_wrapped(|ui| {
                    for (key, swatch) in [
                        ("template-setup-color-none", None),
                        ("template-setup-color-oak", Some(SrgbColor([226, 197, 156]))),
                        (
                            "template-setup-color-white",
                            Some(SrgbColor([244, 242, 238])),
                        ),
                        (
                            "template-setup-color-walnut",
                            Some(SrgbColor([166, 136, 101])),
                        ),
                    ] {
                        ui.horizontal(|ui| {
                            let [r, g, b] = swatch.map_or([215, 210, 201], |color| color.0);
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
                            if ui
                                .selectable_label(material.color == swatch, l.text(key))
                                .clicked()
                            {
                                material.color = swatch;
                            }
                        });
                    }
                });
                ui.small(l.text("template-setup-color-hint"));
                ((), material.valid())
            },
        );
        match result.action {
            ModalAction::Cancel => {
                self.child.close(ctx);
                self.parent.body_replaced(ctx);
                self.restore_material_button = true;
                self.material = None; // parent setup is untouched
            }
            ModalAction::Confirm if material.valid() => {
                // `valid` implies a proposal; the pattern states it for the compiler.
                if let Some(MaterialEntry {
                    name,
                    proposal: Some(proposal),
                    grain,
                    color,
                    ..
                }) = self.material.take()
                {
                    self.setup.add_material(name, proposal, grain, color);
                }
                self.child.close(ctx);
                self.parent.body_replaced(ctx);
                self.restore_material_button = true;
            }
            _ => {}
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 256
}

fn project_fields(ui: &mut egui::Ui, l: &Localizer, setup: &mut TemplateSetup) {
    ui.label(l.text("template-setup-project-name"));
    ui.add(
        egui::TextEdit::singleline(&mut setup.project_name)
            .id(egui::Id::new("template-project-name")),
    );
    if !valid_name(&setup.project_name) {
        ui.colored_label(DANGER, l.text("template-setup-error-name"));
    }
    ui.add_space(12.0);
    ui.label(l.text("template-setup-currency"));
    ui.horizontal_wrapped(|ui| {
        for currency in [Currency::Brl, Currency::Usd] {
            if ui
                .selectable_label(setup.currency == currency, currency.code())
                .clicked()
            {
                setup.currency = currency;
            }
        }
    });
    ui.label(l.text("template-setup-input-unit"));
    ui.horizontal_wrapped(|ui| {
        for unit in [Unit::Mm, Unit::Cm, Unit::M, Unit::Inch, Unit::Foot] {
            if ui
                .selectable_label(setup.input_unit == unit, unit_label(unit))
                .clicked()
            {
                setup.input_unit = unit;
            }
        }
    });
    ui.small(l.text("template-setup-unit-hint"));
}

fn material_roles(
    ui: &mut egui::Ui,
    l: &Localizer,
    setup: &mut TemplateSetup,
    open: &mut bool,
    button_id: &mut Option<egui::Id>,
) {
    ui.label(l.text("template-setup-material-intro"));
    for material in &setup.materials {
        ui.horizontal_wrapped(|ui| {
            if let Some(color) = material.color {
                let [r, g, b] = color.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(13.0, 13.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
            }
            ui.label(format!(
                "{} · {} · {}",
                material.name,
                format_length(
                    material.thickness.conversion.suggested(),
                    Unit::Mm,
                    locale(l),
                    3
                ),
                l.text(grain_key(material.grain))
            ));
        });
    }
    let button = ui.button(l.text("template-setup-material-new"));
    *button_id = Some(button.id);
    if button.clicked() {
        *open = true;
    }
    if setup.materials.is_empty() {
        ui.colored_label(WARN, l.text("template-setup-first-run"));
    }
    ui.add_space(14.0);
    ui.label(RichText::new(l.text("template-setup-roles")).strong());
    for &role in setup.kind.roles() {
        ui.horizontal_wrapped(|ui| {
            ui.label(l.text(role_key(role)));
            let chosen = setup
                .roles
                .get(&role)
                .and_then(|id| setup.materials.iter().find(|m| m.id == *id));
            egui::ComboBox::from_id_salt(("template-role", role))
                .selected_text(chosen.map_or_else(
                    || l.text("template-setup-choose-material"),
                    |m| m.name.clone(),
                ))
                .show_ui(ui, |ui| {
                    for material in &setup.materials {
                        if ui
                            .selectable_label(
                                chosen.is_some_and(|m| m.id == material.id),
                                format!(
                                    "{} · {}",
                                    material.name,
                                    format_length(
                                        material.thickness.conversion.suggested(),
                                        Unit::Mm,
                                        locale(l),
                                        3
                                    )
                                ),
                            )
                            .clicked()
                        {
                            setup.roles.insert(role, material.id);
                        }
                    }
                });
        });
    }
    ui.small(l.text("template-setup-role-hint"));
}

fn dimensions(
    ui: &mut egui::Ui,
    l: &Localizer,
    setup: &mut TemplateSetup,
    fields: &mut BTreeMap<TemplateField, LengthEntry>,
    count: &mut String,
) {
    ui.label(l.text("template-setup-size-intro"));
    for &field in setup.kind.fields() {
        let entry = fields.get_mut(&field).expect("initialized field");
        let mut proposal = setup.dimensions.get(&field).copied();
        length_field(ui, l, field_key(field), entry, &mut proposal);
        match proposal {
            Some(value) => {
                setup.dimensions.insert(field, value);
            }
            None => {
                setup.dimensions.remove(&field);
            }
        }
    }
    if setup.kind == TemplateKind::Drawers {
        ui.label(l.text("template-setup-drawer-count"));
        if ui
            .add(egui::TextEdit::singleline(count).desired_width(100.0))
            .changed()
        {
            setup.drawer_count = count
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=1000).contains(n));
        }
        if setup.drawer_count.is_none() {
            ui.colored_label(DANGER, l.text("template-setup-error-count"));
        }
    }
    ui.small(l.text("template-setup-clearance-hint"));
}

fn length_field(
    ui: &mut egui::Ui,
    l: &Localizer,
    label: &str,
    entry: &mut LengthEntry,
    proposal: &mut Option<ProposedLength>,
) {
    ui.label(l.text(label));
    if ui
        .add(egui::TextEdit::singleline(&mut entry.text).desired_width(190.0))
        .changed()
    {
        *proposal = entry.parse(); // replaces proposal, clearing previous consent
    }
    if entry.invalid || proposal.is_none() {
        ui.colored_label(DANGER, l.text("template-setup-error-length"));
    } else if let Some(proposal) = proposal {
        let suggested = proposal.conversion.suggested();
        if matches!(proposal.conversion, Conversion::NeedsConfirmation(_)) {
            ui.colored_label(
                WARN,
                format!(
                    "{} → {}",
                    entry.text,
                    format_length(suggested, Unit::Mm, locale(l), 3)
                ),
            );
            let mut confirmed = proposal.rounding_confirmed;
            if ui
                .checkbox(&mut confirmed, l.text("template-setup-rounding"))
                .changed()
            {
                proposal.rounding_confirmed = confirmed;
            }
        } else {
            ui.small(format!(
                "{} · {}",
                l.text("template-setup-exact"),
                format_length(suggested, Unit::Mm, locale(l), 3)
            ));
        }
    }
    ui.add_space(6.0);
}

fn grain_field(ui: &mut egui::Ui, l: &Localizer, grain: &mut BoardGrain) {
    ui.label(l.text("template-setup-grain"));
    ui.horizontal_wrapped(|ui| {
        for value in [
            BoardGrain::Length,
            BoardGrain::Width,
            BoardGrain::Unrestricted,
        ] {
            if ui
                .selectable_label(*grain == value, l.text(grain_key(value)))
                .clicked()
            {
                *grain = value;
            }
        }
    });
}

fn review_panel(ui: &mut egui::Ui, l: &Localizer, setup: &TemplateSetup, review: &TemplateReview) {
    ui.label(l.text("template-setup-review-intro"));
    ui.label(format!(
        "{}: {} × {} × {}",
        l.text("template-setup-outer-size"),
        mm(review.candidate.size.width, l),
        mm(review.candidate.size.depth, l),
        mm(review.candidate.size.height, l)
    ));
    datum_diagram(ui, l, review);
    ui.small(l.text(if setup.kind == TemplateKind::Drawers {
        "template-setup-assumptions-drawers"
    } else if setup.kind == TemplateKind::Wall {
        "template-setup-assumptions-wall"
    } else {
        "template-setup-assumptions-base"
    }));
    ui.label(RichText::new(l.text("template-setup-review-inputs")).strong());
    for &field in setup.kind.fields() {
        let value = setup.dimensions[&field].conversion.suggested();
        ui.label(format!("{}: {}", l.text(field_key(field)), mm(value, l)));
    }
    if let Some(count) = setup.drawer_count {
        ui.label(format!(
            "{}: {count}",
            l.text("template-setup-drawer-count")
        ));
    }
    ui.label(RichText::new(l.text("template-setup-review-bom")).strong());
    for (index, board) in review.candidate.boards.iter().enumerate() {
        let material = review
            .materials
            .iter()
            .find(|m| m.id == board.material_id)
            .expect("review material");
        let fit = match &review.fits[index] {
            (FirstFit::Allocated(_), Some(allocation)) => format!(
                "{} · {} X {} Y {}",
                l.text("template-setup-fit"),
                allocation.stock_id,
                mm(allocation.origin[0], l),
                mm(allocation.origin[1], l)
            ),
            (FirstFit::SearchExhausted, _) => l.text("template-setup-fit-unknown"),
            _ => l.text("template-setup-unallocated"),
        };
        egui::Frame::new()
            .fill(Color32::WHITE)
            .stroke(egui::Stroke::new(1.0, BORDER))
            .corner_radius(6)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.label(RichText::new(board_label(&board.name, l)).strong());
                ui.small(format!(
                    "{} · {} × {} × {} · {} · {fit}",
                    material.name,
                    mm(board.length, l),
                    mm(board.width, l),
                    mm(board.thickness, l),
                    l.text(grain_key(board.effective_grain(material)))
                ));
                let [x, y, z] = board
                    .pose
                    .translation_mm
                    .map(|value| mm(Length::from_micrometres((value * 1000.0).round() as i64), l));
                ui.small(format!(
                    "{} (X {x}, Y {y}, Z {z}) · {} L→{} W→{} T→{}",
                    l.text("template-setup-board-origin"),
                    l.text("template-setup-board-orientation"),
                    axis(board.pose.rotation.rotate([1.0, 0.0, 0.0])),
                    axis(board.pose.rotation.rotate([0.0, 1.0, 0.0])),
                    axis(board.pose.rotation.rotate([0.0, 0.0, 1.0]))
                ));
            });
    }
    ui.colored_label(WARN, l.text("template-setup-stock-next"));
    ui.small(l.text("template-setup-disclaimer"));
}

fn axis(vector: [f64; 3]) -> &'static str {
    let index = (0..3)
        .max_by(|&a, &b| vector[a].abs().total_cmp(&vector[b].abs()))
        .expect("three axes");
    match (index, vector[index].is_sign_negative()) {
        (0, false) => "+X",
        (0, true) => "−X",
        (1, false) => "+Y",
        (1, true) => "−Y",
        (2, false) => "+Z",
        (2, true) => "−Z",
        _ => unreachable!(),
    }
}

fn datum_diagram(ui: &mut egui::Ui, l: &Localizer, review: &TemplateReview) {
    ui.label(RichText::new(l.text("template-setup-datum-title")).strong());
    let datums = review.datums;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(120.0), 78.0),
        egui::Sense::hover(),
    );
    let min = datums.front_outside.micrometres();
    let max = datums.back_outside.micrometres();
    let span = (max - min).max(1) as f32;
    let x = |value: Length| {
        rect.left()
            + 22.0
            + (value.micrometres() - min) as f32 / span * (rect.width() - 44.0).max(1.0)
    };
    let paint = ui.painter();
    paint.line_segment(
        [
            egui::pos2(rect.left() + 22.0, rect.center().y),
            egui::pos2(rect.right() - 22.0, rect.center().y),
        ],
        egui::Stroke::new(2.0, MUTED),
    );
    for (value, letter) in [
        (datums.front_outside, "F"),
        (datums.carcass_front, "C"),
        (datums.carcass_rear, "R"),
        (datums.back_outside, "B"),
    ] {
        let px = x(value);
        paint.line_segment(
            [
                egui::pos2(px, rect.center().y - 7.0),
                egui::pos2(px, rect.center().y + 7.0),
            ],
            egui::Stroke::new(1.0, WARN),
        );
        paint.text(
            egui::pos2(px, rect.center().y - 10.0),
            egui::Align2::CENTER_BOTTOM,
            letter,
            egui::FontId::proportional(10.0),
            MUTED,
        );
    }
    for (letter, value, key) in [
        ("F", datums.front_outside, "template-setup-datum-front"),
        (
            "C",
            datums.carcass_front,
            "template-setup-datum-carcass-front",
        ),
        (
            "R",
            datums.carcass_rear,
            "template-setup-datum-carcass-rear",
        ),
        ("B", datums.back_outside, "template-setup-datum-back"),
    ] {
        ui.small(format!("{letter} · {}: {}", l.text(key), mm(value, l)));
    }
    ui.small(l.text("template-setup-datum-hint"));
}

fn board_label(name: &str, l: &Localizer) -> String {
    if let Some(rest) = name.strip_prefix("Drawer ")
        && let Some((number, part)) = rest.split_once(' ')
    {
        let key = match part {
            "left box side" => Some("template-setup-part-box-left"),
            "right box side" => Some("template-setup-part-box-right"),
            "box front" => Some("template-setup-part-box-front"),
            "box back" => Some("template-setup-part-box-back"),
            "applied bottom" => Some("template-setup-part-applied-bottom"),
            "external front" => Some("template-setup-part-external-front"),
            _ => None,
        };
        if let Some(key) = key {
            return format!(
                "{} {number} · {}",
                l.text("template-setup-drawer"),
                l.text(key)
            );
        }
    }
    let key = match name {
        "Left side" => Some("template-setup-part-left"),
        "Right side" => Some("template-setup-part-right"),
        "Bottom" => Some("template-setup-part-bottom"),
        "Top" => Some("template-setup-part-top"),
        "Shelf" => Some("template-setup-part-shelf"),
        "Front top rail" => Some("template-setup-part-front-rail"),
        "Rear top rail" => Some("template-setup-part-rear-rail"),
        "Overlay back" => Some("template-setup-part-back"),
        _ => None,
    };
    key.map_or_else(|| name.to_owned(), |key| l.text(key))
}

fn errors_panel(ui: &mut egui::Ui, l: &Localizer, setup: &TemplateSetup, errors: &[SetupError]) {
    ui.colored_label(DANGER, l.text("template-setup-review-blocked"));
    for error in errors {
        let detail = match error {
            SetupError::ProjectName => l.text("template-setup-project-name"),
            SetupError::MaterialName(id)
            | SetupError::DuplicateMaterial(id)
            | SetupError::MaterialThickness(id)
            | SetupError::MaterialRounding(id) => setup
                .materials
                .iter()
                .find(|m| m.id == *id)
                .map_or_else(|| id.to_string(), |m| m.name.clone()),
            SetupError::MissingField(field) | SetupError::Rounding(field) => {
                l.text(field_key(*field))
            }
            SetupError::MissingCount => l.text("template-setup-drawer-count"),
            SetupError::MissingRole(role) | SetupError::UnknownMaterial(role) => {
                l.text(role_key(*role))
            }
            SetupError::Geometry(errors) => {
                for error in errors {
                    ui.colored_label(DANGER, recipe_error(l, error));
                }
                continue;
            }
            SetupError::InvalidProject(_) => l.text("template-setup-error-project"),
        };
        let key = match error {
            SetupError::Rounding(_) | SetupError::MaterialRounding(_) => {
                "template-setup-error-rounding"
            }
            SetupError::MissingRole(_) | SetupError::UnknownMaterial(_) => {
                "template-setup-error-role"
            }
            SetupError::MissingField(_) | SetupError::MissingCount => {
                "template-setup-error-missing"
            }
            _ => "template-setup-error-invalid",
        };
        ui.colored_label(DANGER, format!("{}: {detail}", l.text(key)));
    }
}

fn recipe_error(l: &Localizer, error: &RecipeError) -> String {
    let kind = match error.kind {
        RecipeErrorKind::NonPositive => "template-setup-geometry-positive",
        RecipeErrorKind::Negative => "template-setup-geometry-negative",
        RecipeErrorKind::Geometry => "template-setup-geometry-space",
        RecipeErrorKind::OutOfBounds => "template-setup-geometry-bounds",
        RecipeErrorKind::TooMany => "template-setup-geometry-count",
        RecipeErrorKind::MaterialConflict => "template-setup-geometry-material",
    };
    let field = match error.field {
        "width" => "template-setup-width",
        "depth" => "template-setup-depth",
        "height" => "template-setup-height",
        "rail_width" => "template-setup-rail-width",
        "shelf_height" => "template-setup-shelf-height",
        "box_depth" => "template-setup-box-depth",
        "side_clearance" | "box_width" | "carcass_opening_width" | "box_front_back_length" => {
            "template-setup-side-clearance"
        }
        "rear_clearance" | "carcass_depth" | "box_inner_depth" => "template-setup-rear-clearance",
        "vertical_clearance" | "box_height" | "box_side_height" | "carcass_opening_height" => {
            "template-setup-vertical-clearance"
        }
        "front_reveal" | "front_height" | "front_width" => "template-setup-front-reveal",
        "front_gap" => "template-setup-front-gap",
        "count" => "template-setup-drawer-count",
        "carcass_thickness" => "template-setup-role-carcass",
        "back_thickness" => "template-setup-role-back",
        "box_thickness" => "template-setup-role-box",
        "box_bottom_thickness" => "template-setup-role-bottom",
        "front_thickness" => "template-setup-role-front",
        _ => "template-setup-geometry-derived",
    };
    format!("{}: {}", l.text(field), l.text(kind))
}

fn mm(value: Length, l: &Localizer) -> String {
    format_length(value, Unit::Mm, locale(l), 3)
}

fn locale(l: &Localizer) -> Locale {
    match l.language() {
        Language::En => Locale::En,
        Language::PtBr => Locale::PtBr,
    }
}

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "mm",
        Unit::Cm => "cm",
        Unit::M => "m",
        Unit::Inch => "in",
        Unit::Foot => "ft",
    }
}

fn kind_key(kind: TemplateKind) -> &'static str {
    match kind {
        TemplateKind::Base => "template-setup-base",
        TemplateKind::Wall => "template-setup-wall",
        TemplateKind::Drawers => "template-setup-drawers",
    }
}

fn stage_key(stage: Stage) -> &'static str {
    match stage {
        Stage::Project => "template-setup-stage-project",
        Stage::Materials => "template-setup-stage-materials",
        Stage::Dimensions => "template-setup-stage-dimensions",
        Stage::Review => "template-setup-stage-review",
    }
}

fn role_key(role: MaterialRole) -> &'static str {
    match role {
        MaterialRole::Carcass => "template-setup-role-carcass",
        MaterialRole::Back => "template-setup-role-back",
        MaterialRole::Box => "template-setup-role-box",
        MaterialRole::BoxBottom => "template-setup-role-bottom",
        MaterialRole::ExternalFront => "template-setup-role-front",
    }
}

fn field_key(field: TemplateField) -> &'static str {
    match field {
        TemplateField::Width => "template-setup-width",
        TemplateField::Depth => "template-setup-depth",
        TemplateField::Height => "template-setup-height",
        TemplateField::RailWidth => "template-setup-rail-width",
        TemplateField::ShelfHeight => "template-setup-shelf-height",
        TemplateField::BoxDepth => "template-setup-box-depth",
        TemplateField::SideClearance => "template-setup-side-clearance",
        TemplateField::RearClearance => "template-setup-rear-clearance",
        TemplateField::VerticalClearance => "template-setup-vertical-clearance",
        TemplateField::FrontReveal => "template-setup-front-reveal",
        TemplateField::FrontGap => "template-setup-front-gap",
    }
}

fn grain_key(grain: BoardGrain) -> &'static str {
    match grain {
        BoardGrain::Length => "template-setup-grain-length",
        BoardGrain::Width => "template-setup-grain-width",
        BoardGrain::Unrestricted => "template-setup-grain-any",
    }
}

#[cfg(test)]
mod tests;
