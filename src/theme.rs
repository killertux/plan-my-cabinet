//! Offline typography for the native workspace redesign (egui 0.36.2).

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

/// A real static face, not synthetic emboldening of the regular font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typeface {
    Sans,
    SansMedium,
    SansSemibold,
    Mono,
    MonoMedium,
    MonoSemibold,
}

impl Typeface {
    pub fn family(self) -> FontFamily {
        match self {
            Self::Sans => FontFamily::Proportional,
            Self::SansMedium => FontFamily::Name("noto-medium".into()),
            Self::SansSemibold => FontFamily::Name("noto-semibold".into()),
            Self::Mono => FontFamily::Monospace,
            Self::MonoMedium => FontFamily::Name("jetbrains-medium".into()),
            Self::MonoSemibold => FontFamily::Name("jetbrains-semibold".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextToken {
    pub size: f32,
    pub typeface: Typeface,
}

impl TextToken {
    const fn new(size: f32, typeface: Typeface) -> Self {
        Self { size, typeface }
    }

    pub fn font_id(self) -> FontId {
        FontId::new(self.size, self.typeface.family())
    }
}

pub const HEADING: TextToken = TextToken::new(20.0, Typeface::SansSemibold);
pub const PAGE_TITLE: TextToken = TextToken::new(17.0, Typeface::SansSemibold);
pub const TITLE: TextToken = TextToken::new(15.0, Typeface::SansSemibold);
pub const BODY: TextToken = TextToken::new(13.0, Typeface::Sans);
pub const BODY_MEDIUM: TextToken = TextToken::new(13.0, Typeface::SansMedium);
pub const BUTTON: TextToken = TextToken::new(13.0, Typeface::SansMedium);
pub const PRIMARY_BUTTON: TextToken = TextToken::new(13.0, Typeface::SansSemibold);
pub const PART_LABEL: TextToken = TextToken::new(12.5, Typeface::SansMedium);
pub const SMALL: TextToken = TextToken::new(12.0, Typeface::Sans);
pub const STATUS: TextToken = TextToken::new(11.5, Typeface::Sans);
pub const CAPTION: TextToken = TextToken::new(11.0, Typeface::Sans);
pub const SECTION: TextToken = TextToken::new(11.0, Typeface::SansSemibold);
pub const SECTION_COMPACT: TextToken = TextToken::new(10.5, Typeface::SansSemibold);
/// Multiply by the section token's point size for LayoutJob extra_letter_spacing.
pub const SECTION_TRACKING_EM: f32 = 0.08;
pub const MONO: TextToken = TextToken::new(13.0, Typeface::Mono);
pub const MONO_SMALL: TextToken = TextToken::new(12.0, Typeface::Mono);
pub const MONO_MEDIUM: TextToken = TextToken::new(12.0, Typeface::MonoMedium);
pub const MONO_ID: TextToken = TextToken::new(11.0, Typeface::Mono);
pub const SHORTCUT: TextToken = TextToken::new(11.0, Typeface::Mono);
pub const CUT_MARKER: TextToken = TextToken::new(10.0, Typeface::MonoSemibold);
pub const RAIL: TextToken = TextToken::new(9.5, Typeface::Sans);
pub const RAIL_ACTIVE: TextToken = TextToken::new(9.5, Typeface::SansMedium);

const NAMED_STYLES: &[(&str, TextToken)] = &[
    ("PageTitle", PAGE_TITLE),
    ("Title", TITLE),
    ("BodyMedium", BODY_MEDIUM),
    ("PrimaryButton", PRIMARY_BUTTON),
    ("PartLabel", PART_LABEL),
    ("Status", STATUS),
    ("Caption", CAPTION),
    ("Section", SECTION),
    ("SectionCompact", SECTION_COMPACT),
    ("MonoSmall", MONO_SMALL),
    ("MonoMedium", MONO_MEDIUM),
    ("MonoId", MONO_ID),
    ("Shortcut", SHORTCUT),
    ("CutMarker", CUT_MARKER),
    ("Rail", RAIL),
    ("RailActive", RAIL_ACTIVE),
];

const FACES: &[(Typeface, &str, &[u8])] = &[
    (
        Typeface::Sans,
        "NotoSans-Regular",
        include_bytes!("../assets/fonts/NotoSans-Regular.ttf"),
    ),
    (
        Typeface::SansMedium,
        "NotoSans-Medium",
        include_bytes!("../assets/fonts/NotoSans-Medium.ttf"),
    ),
    (
        Typeface::SansSemibold,
        "NotoSans-SemiBold",
        include_bytes!("../assets/fonts/NotoSans-SemiBold.ttf"),
    ),
    (
        Typeface::Mono,
        "JetBrainsMono-Regular",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    ),
    (
        Typeface::MonoMedium,
        "JetBrainsMono-Medium",
        include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    ),
    (
        Typeface::MonoSemibold,
        "JetBrainsMono-SemiBold",
        include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
    ),
];

fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    // Preserve bundled egui symbol/emoji coverage after each intended primary face.
    let sans_fallback = fonts.families[&FontFamily::Proportional].clone();
    let mono_fallback = fonts.families[&FontFamily::Monospace].clone();
    for &(typeface, name, bytes) in FACES {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
        let fallback = match typeface {
            Typeface::Sans | Typeface::SansMedium | Typeface::SansSemibold => &sans_fallback,
            Typeface::Mono | Typeface::MonoMedium | Typeface::MonoSemibold => &mono_fallback,
        };
        let mut family = vec![name.into()];
        // Noto Sans lacks several arrows and macOS modifier symbols. Use the
        // corresponding real Mono weight before egui's general fallback fonts.
        let symbol_face = match typeface {
            Typeface::Sans => Some("JetBrainsMono-Regular"),
            Typeface::SansMedium => Some("JetBrainsMono-Medium"),
            Typeface::SansSemibold => Some("JetBrainsMono-SemiBold"),
            _ => None,
        };
        if let Some(symbol_face) = symbol_face {
            family.push(symbol_face.into());
        }
        family.extend(fallback.iter().cloned());
        fonts.families.insert(typeface.family(), family);
    }
    fonts
}

/// Call once during application creation, before the first UI frame.
/// Installs embedded faces and typography only; all sizes are logical points.
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    ctx.all_styles_mut(|style| {
        for (name, token) in [
            (TextStyle::Heading, HEADING),
            (TextStyle::Body, BODY),
            (TextStyle::Button, BUTTON),
            (TextStyle::Small, SMALL),
            (TextStyle::Monospace, MONO),
        ] {
            style.text_styles.insert(name, token.font_id());
        }
        for &(name, token) in NAMED_STYLES {
            style
                .text_styles
                .insert(TextStyle::Name(name.into()), token.font_id());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCALIZED: &str =
        "Ação Português Dimensões Peça Espessura ÁÀÂÃÉÊÍÓÔÕÚÜÇ áàâãéêíóôõúüç 0123456789 × ° Ø ² …";

    #[test]
    fn embedded_faces_have_real_weights_and_localized_glyphs() {
        for (index, &(_, name, bytes)) in FACES.iter().enumerate() {
            let face = ttf_parser::Face::parse(bytes, 0).expect(name);
            assert_eq!(
                face.weight().to_number(),
                [400, 500, 600][index % 3],
                "{name}"
            );
            assert!(!face.is_variable(), "static weight required: {name}");
            for ch in LOCALIZED.chars() {
                assert!(face.glyph_index(ch).is_some(), "{name} missing {ch}");
            }
            if index >= 3 {
                let advance = face.glyph_hor_advance(face.glyph_index('0').unwrap());
                for ch in "123456789.,+-".chars() {
                    assert_eq!(
                        face.glyph_hor_advance(face.glyph_index(ch).unwrap()),
                        advance,
                        "{name}: {ch}"
                    );
                }
            }
            for &(_, other_name, other_bytes) in &FACES[..index] {
                assert_ne!(bytes, other_bytes, "duplicate faces {name}/{other_name}");
            }
        }
    }

    #[test]
    fn weighted_families_start_with_their_own_face() {
        let fonts = font_definitions();
        for &(typeface, name, _) in FACES {
            let family = &fonts.families[&typeface.family()];
            assert_eq!(family[0], name);
            assert!(family.len() > 1, "keep offline symbol fallbacks");
        }
    }

    #[test]
    fn installed_styles_and_shortcuts_work_without_network() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            assert_eq!(
                ui.style().text_styles[&TextStyle::Heading],
                HEADING.font_id()
            );
            for &(name, token) in NAMED_STYLES {
                assert_eq!(
                    ui.style().text_styles[&TextStyle::Name(name.into())],
                    token.font_id()
                );
            }
            ctx.fonts_mut(|fonts| {
                let mut missing = Vec::new();
                for &(typeface, name, _) in FACES {
                    let id = FontId::new(13.0, typeface.family());
                    assert!(fonts.has_glyphs(&id, LOCALIZED), "{name}");
                    for ch in "⌘⌥⇧⌃⏎←↑→↓ Esc Ctrl Alt + −".chars() {
                        if !fonts.has_glyph(&id, ch) {
                            missing.push(format!("{name}: {ch}"));
                        }
                    }
                }
                assert!(missing.is_empty(), "missing shortcuts: {missing:?}");
            });
        });
        output.textures_delta.clear();
    }
}
