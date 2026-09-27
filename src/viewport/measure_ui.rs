//! One read-only bounds readout for the viewport and Design inspector.
use super::Selection;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::Localizer;
use plan_my_cabinet::measurements::{Frame, MeasurementError, Scope, measure};
use plan_my_cabinet::units::Unit;

pub(crate) fn measurement_readout(
    project: &Project,
    selection: &Selection,
    scope: Scope,
    frame: Frame,
    localizer: &Localizer,
) -> (String, String) {
    let scope_label = localizer.text(match scope {
        Scope::Body => "measurement-body",
        Scope::Overall => "measurement-overall",
    });
    let frame_label = match frame {
        Frame::World => localizer.text("placement-world"),
        Frame::Object(id) => project
            .assemblies
            .iter()
            .map(|a| (a.id, a.name.as_str()))
            .chain(project.boards.iter().map(|b| (b.id, b.name.as_str())))
            .find(|(object, _)| *object == id)
            .map(|(_, name)| format!("{name} ({})", &id.to_string()[..8]))
            .unwrap_or_else(|| localizer.text("measurement-invalid")),
    };
    let heading = format!(
        "{scope_label} · {}: {frame_label}",
        localizer.text("measurement-frame")
    );
    let ids: Vec<_> = selection.ids.iter().copied().collect();
    let content = match measure(project, &ids, scope, frame) {
        Ok(result) => {
            // Bounds are continuous f64 spatial values, not manufacturing-grid
            // quantities. Convert only for presentation; never round the model.
            let (factor, suffix) = match project.display_unit {
                Unit::Mm => (1.0, "mm"),
                Unit::Cm => (10.0, "cm"),
                Unit::M => (1000.0, "m"),
                Unit::Inch => (25.4, "in"),
                Unit::Foot => (304.8, "ft"),
            };
            let numbers = result.dimensions_mm.map(|mm| {
                let text = format!("{:.3}", mm / factor);
                if localizer.language() == plan_my_cabinet::i18n::Language::PtBr {
                    text.replace('.', ",")
                } else {
                    text
                }
            });
            format!(
                "X × Y × Z: {} × {} × {} {suffix}",
                numbers[0], numbers[1], numbers[2]
            )
        }
        Err(MeasurementError::EmptySelection) => localizer.text("measurement-empty"),
        Err(MeasurementError::UndimensionedHardware(_)) => {
            localizer.text("measurement-unknown-hardware")
        }
        Err(_) => localizer.text("measurement-invalid"),
    };
    (heading, content)
}
