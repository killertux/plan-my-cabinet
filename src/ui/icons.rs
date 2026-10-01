//! Bundled handoff icons, converted to white-alpha masks for multiplicative tint.
//! See `docs/redesign-icons.md` for source provenance and verification.

use eframe::egui::{self, Color32, Image, ImageSource};

macro_rules! icons {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// The original 40-icon handoff set, plus icons drawn for the app in
        /// the same style (see `APP_ICONS`).
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum Icon { $($variant),+ }

        impl Icon {
            pub const ALL: [Self; 42] = [$(Self::$variant),+];

            /// Stable asset identifier, not a localized action label.
            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            /// Embedded SVG source; URI and raster cache are shared across tints.
            pub fn source(self) -> ImageSource<'static> {
                match self {
                    $(Self::$variant => egui::include_image!(concat!("../../assets/icons/", $name, ".svg"))),+
                }
            }
        }
    };
}

icons! {
    Orbit => "orbit", Move => "move", Measure => "measure", Frame => "frame",
    Board => "board", Assembly => "assembly", Material => "material", Sheet => "sheet",
    Cut => "cut", Hinge => "hinge", Export => "export", Undo => "undo", Redo => "redo",
    Eye => "eye", EyeOff => "eyeoff", Lock => "lock", Plus => "plus", Grid => "grid",
    Magnet => "magnet", Search => "search", Sliders => "sliders", Warning => "warning",
    Check => "check", ChevDown => "chevdown", ChevRight => "chevright",
    Duplicate => "duplicate", Trash => "trash", Save => "save", Folder => "folder",
    Place => "place", Axes => "axes", Bolt => "bolt", Layers => "layers", Globe => "globe",
    Door => "door", Cube => "cube", Dots => "dots", List => "list", Command => "command",
    Grain => "grain", Band => "band", Light => "light",
}

/// Icons drawn for the app after the handoff, in its style (24 × 24,
/// 1.7 stroke, round caps and joins). They have no handoff source.
pub const APP_ICONS: &[&str] = &["band", "light"];

/// Install the pinned SVG loader once during app creation (safe to call again).
/// SVG bytes are embedded; no filesystem or HTTP loader is enabled.
pub fn install_loaders(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// A square icon in logical points, with the caller's exact color/alpha.
///
/// Wrap in `egui::Button::image` for keyboard/focus behavior and supply a
/// localized action name with `.alt_text(label)` (not `Icon::name()`).
/// `Color32::from_rgba_unmultiplied(r, g, b, 102)` gives 40% opacity.
pub fn icon(symbol: Icon, color: Color32, size: f32) -> Image<'static> {
    Image::new(symbol.source())
        .fit_to_exact_size(egui::Vec2::splat(size))
        .tint(color)
}
