//! Bundled Fluent messages. Presentation language is chosen per localizer, so
//! callers can keep separate UI and export localizers without changing data.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

const ENGLISH: &str = include_str!("../i18n/en.ftl");
const PORTUGUESE_BRAZIL: &str = include_str!("../i18n/pt-BR.ftl");

/// Initial shared vocabulary for the desktop shell, validation and outputs.
/// Tests enforce both resources cover these keys; later workflows can extend it.
pub const SHARED_KEYS: &[&str] = &[
    "app-title",
    "language-en",
    "language-pt-br",
    "ui-language",
    "export-language",
    "export-output-units",
    "export-choose",
    "export-working",
    "export-choosing",
    "export-cancelled",
    "export-overwrite",
    "export-replace",
    "export-saved",
    "export-saved-stale",
    "export-failed",
    "export-durability",
    "export-verify-failed",
    "export-render-failed",
    "export-write-failed",
    "project-new",
    "project-open",
    "project-save",
    "project-save-as",
    "project-default-name",
    "project-close",
    "project-unsaved-title",
    "project-unsaved-detail",
    "project-discard",
    "project-cancelled",
    "project-choosing",
    "project-saved",
    "project-overwrite",
    "project-replace",
    "project-recovery-title",
    "project-saved-revision",
    "project-recovery-revision",
    "project-recover",
    "project-recovery-discard",
    "project-defer",
    "project-recovery-unavailable",
    "board-new",
    "board-name",
    "board-length",
    "board-width",
    "board-thickness",
    "board-edit-dimension",
    "board-local-dimension",
    "board-resize-anchor",
    "board-dimension-preview",
    "error-board-dimension",
    "material-new",
    "material-name",
    "material-grain",
    "material-list",
    "material-edit",
    "material-anchor",
    "anchor-start",
    "anchor-centre",
    "anchor-end",
    "material-affected",
    "material-apply-error",
    "material-prospective-conflict",
    "material-preserve",
    "material-apply-all",
    "material-apply-blocked",
    "material-post-conflicts",
    "error-material-edit",
    "conflict-material-identity",
    "conflict-thickness",
    "conflict-grain",
    "conflict-outside-stock",
    "conflict-overlap",
    "grain-length",
    "grain-width",
    "grain-unrestricted",
    "board-effective",
    "board-unallocated",
    "board-list",
    "board-input-hint",
    "error-material-missing",
    "error-create",
    "material",
    "stock",
    "stock-list",
    "cutting-kerf",
    "cutting-kerf-edit",
    "cutting-kerf-invalid",
    "cutting-kerf-hint",
    "first-fit-allocated",
    "first-fit-no-fit",
    "first-fit-exhausted",
    "stock-new",
    "stock-edit",
    "stock-up",
    "stock-down",
    "stock-name",
    "stock-grain",
    "stock-grain-x",
    "stock-grain-y",
    "stock-grain-none",
    "stock-grain-unknown",
    "stock-source",
    "stock-owned",
    "stock-purchase",
    "stock-price",
    "stock-price-unknown",
    "stock-quantity",
    "stock-trim-left",
    "stock-trim-right",
    "stock-trim-bottom",
    "stock-trim-top",
    "stock-trim-hint",
    "stock-invalid-trim",
    "stock-invalid-quantity",
    "stock-invalid",
    "stock-stale",
    "assembly",
    "allocation",
    "cut-plan",
    "estimate",
    "export",
    "export-never",
    "export-current",
    "export-stale",
    "pdf-packet",
    "pdf-project",
    "pdf-revision",
    "pdf-units",
    "pdf-currency",
    "pdf-kerf",
    "pdf-assumptions",
    "pdf-assumptions-detail",
    "pdf-issues",
    "pdf-stock",
    "pdf-price",
    "pdf-trim",
    "pdf-parts",
    "pdf-hardware",
    "hardware-list",
    "hardware-new",
    "hardware-edit",
    "hardware-duplicate",
    "hardware-name",
    "hardware-dimensions",
    "hardware-position",
    "hardware-invalid",
    "pdf-installation-withheld",
    "pdf-sheet",
    "pdf-scale",
    "pdf-not-template",
    "pdf-cuts",
    "pdf-retained",
    "pdf-cost",
    "pdf-material-cost",
    "pdf-cut-cost",
    "pdf-total",
    "pdf-incomplete",
    "pdf-part",
    "pdf-offcut",
    "pdf-waste",
    "pdf-used",
    "pdf-edges",
    "pdf-missing",
    "pdf-reference-hardware",
    "pdf-catalog",
    "pdf-hardware-missing-catalog",
    "pdf-hardware-unverified",
    "pdf-hardware-invalid",
    "pdf-invalid-wood",
    "cancel",
    "confirm",
    "undo",
    "redo",
    "viewport-heading",
    "viewport-axes",
    "error-graphics-startup",
    "error-invalid-number",
    "error-grouping-separators",
    "error-invalid-fraction",
    "error-fraction-requires-inches",
    "error-overflow",
    "error-non-positive-dimension",
    "error-out-of-bounds",
    "error-non-finite",
    "error-invalid-rotation",
    "error-invalid-amount",
    "error-negative-amount",
    "error-currency-mismatch",
    "error-invalid-stock-index",
    "error-duplicate-stock",
    "error-replacement-count-mismatch",
    "error-missing-replacement",
    "error-no-edit",
    "rounding-confirmation",
    "error-save",
    "error-open",
    "error-unknown-message",
    "unit-mm",
    "unit-cm",
    "unit-m",
    "unit-in",
    "unit-ft",
    "parts-count",
    "boards-count",
    "placement-numeric",
    "placement-face",
    "placement-source",
    "placement-target",
    "placement-source-face",
    "placement-target-face",
    "placement-frame",
    "placement-local",
    "placement-world",
    "placement-position",
    "placement-rotation",
    "placement-align",
    "placement-offset",
    "placement-gap",
    "placement-preview",
    "placement-off-grid",
    "placement-invalid",
    "face-length-minus",
    "face-length-plus",
    "face-width-minus",
    "face-width-plus",
    "face-thickness-minus",
    "face-thickness-plus",
    "cuts-count",
    "optimize-heading",
    "optimize-spending",
    "optimize-cuts",
    "optimize-area",
    "optimize-start",
    "optimize-cancel",
    "optimize-accept",
    "optimize-source",
    "optimize-progress",
    "optimize-current",
    "optimize-best-found",
    "optimize-heuristic",
    "optimize-cost-no-claim",
    "optimize-stock-used",
    "optimize-cuts-count",
    "optimize-offcuts",
    "optimize-rectangles",
    "optimize-unused",
    "optimize-loss",
    "optimize-cost",
    "optimize-unknown",
    "optimize-placements",
    "optimize-changes",
    "optimize-current-unverified",
    "optimize-unverified",
    "optimize-no-complete",
    "optimize-exhausted",
    "optimize-stale",
    "optimize-blocked",
    "optimize-cancelled",
    "optimize-error",
    "optimize-apply-error",
    "optimize-applied",
    "optimize-unchanged",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    En,
    PtBr,
}

impl Language {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::PtBr => "pt-BR",
        }
    }
}

pub struct Localizer {
    language: Language,
    english: FluentBundle<FluentResource>,
    portuguese: FluentBundle<FluentResource>,
}

impl Localizer {
    pub fn new(language: Language) -> Self {
        Self {
            language,
            english: bundle(Language::En, ENGLISH),
            portuguese: bundle(Language::PtBr, PORTUGUESE_BRAZIL),
        }
    }

    pub const fn language(&self) -> Language {
        self.language
    }

    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    pub fn text(&self, key: &str) -> String {
        self.format(key, None)
    }

    /// Formats Fluent arguments, including numeric selectors for plural forms.
    /// Missing or malformed translations fall back to English. An unknown key
    /// yields an English explanation rather than leaking an internal identifier.
    pub fn format(&self, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        if self.language == Language::PtBr
            && let Some(message) = formatted(&self.portuguese, key, args)
        {
            return message;
        }
        formatted(&self.english, key, args).unwrap_or_else(|| {
            formatted(&self.english, "error-unknown-message", None)
                .expect("bundled English unknown-message text")
        })
    }

    pub fn count(&self, key: &str, count: u64) -> String {
        let mut args = FluentArgs::new();
        args.set("count", count as f64);
        self.format(key, Some(&args))
    }
}

fn bundle(language: Language, source: &str) -> FluentBundle<FluentResource> {
    let locale: LanguageIdentifier = language.tag().parse().expect("valid bundled locale");
    let mut bundle = FluentBundle::new(vec![locale]);
    // Both shipped languages are left-to-right; keep inserted values suitable
    // for plain labels, diagnostics and PDF text without bidi control marks.
    bundle.set_use_isolating(false);
    let resource = FluentResource::try_new(source.to_owned()).unwrap_or_else(|(_, errors)| {
        panic!("invalid {} Fluent resource: {errors:?}", language.tag())
    });
    bundle
        .add_resource(resource)
        .expect("unique bundled messages");
    bundle
}

fn formatted(
    bundle: &FluentBundle<FluentResource>,
    key: &str,
    args: Option<&FluentArgs<'_>>,
) -> Option<String> {
    let pattern = bundle.get_message(key)?.value()?;
    let mut errors = Vec::new();
    let result = bundle
        .format_pattern(pattern, args, &mut errors)
        .into_owned();
    errors.is_empty().then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_keys_exist_in_both_resources() {
        let localizer = Localizer::new(Language::En);
        for key in SHARED_KEYS {
            assert!(
                localizer
                    .english
                    .get_message(key)
                    .and_then(|m| m.value())
                    .is_some(),
                "en: {key}"
            );
            assert!(
                localizer
                    .portuguese
                    .get_message(key)
                    .and_then(|m| m.value())
                    .is_some(),
                "pt-BR: {key}"
            );
        }
    }

    #[test]
    fn plural_forms_follow_the_selected_language() {
        let mut localizer = Localizer::new(Language::En);
        assert_eq!(localizer.count("cuts-count", 1), "1 cut");
        assert_eq!(localizer.count("cuts-count", 2), "2 cuts");
        localizer.set_language(Language::PtBr);
        assert_eq!(localizer.count("cuts-count", 1), "1 corte");
        assert_eq!(localizer.count("cuts-count", 2), "2 cortes");
        assert_eq!(localizer.count("parts-count", 0), "0 peças");
    }

    #[test]
    fn accents_parameters_and_missing_portuguese_use_readable_english() {
        let mut localizer = Localizer::new(Language::PtBr);
        assert_eq!(localizer.text("allocation"), "Alocação");
        assert_eq!(localizer.text("language-pt-br"), "Português brasileiro");
        let mut args = FluentArgs::new();
        args.set("entered", "1/64 in");
        args.set("rounded", "0,397 mm");
        assert_eq!(
            localizer.format("rounding-confirmation", Some(&args)),
            "Arredondar 1/64 in para 0,397 mm?"
        );
        localizer.portuguese = bundle(Language::PtBr, "cancel = Cancelar");
        assert_eq!(localizer.text("project-save"), "Save");
        assert_eq!(
            localizer.text("does-not-exist"),
            "This message is unavailable."
        );
    }

    #[test]
    fn bundled_font_contains_portuguese_and_english_ui_glyphs() {
        let face =
            ttf_parser::Face::parse(include_bytes!("../assets/fonts/NotoSans-Regular.ttf"), 0)
                .expect("bundled font is valid TrueType");
        for text in [ENGLISH, PORTUGUESE_BRAZIL] {
            for c in text.chars().filter(|c| !c.is_control()) {
                // FTL source contains only Latin text and punctuation. Every
                // printed character must be displayable offline.
                assert!(face.glyph_index(c).is_some(), "missing glyph: {c}");
            }
        }
    }
}
