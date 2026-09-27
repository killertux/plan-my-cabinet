//! Renderer-independent A4 packet geometry. Coordinates are millimetres from the
//! top-left of the paper; text glyph positions are resolved before either renderer runs.
//!
//! This model does not read the live project. Callers supply localized, frozen
//! strings and witness-derived geometry.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId};

pub const LAYOUT_VERSION: u32 = 2;
/// Bump when fonts, shaping settings or the fixed measurement scale change.
/// Version 2 separates zero-advance ligature continuations within their
/// original shaped run width for both native preview and PDF serialization.
pub const FONT_METRICS_VERSION: u32 = 2;
pub const A4_WIDTH_MM: f32 = 210.0;
pub const A4_HEIGHT_MM: f32 = 297.0;
pub const MARGIN_MM: f32 = 14.0;
const TOP_MM: f32 = 23.0;
const BOTTOM_MM: f32 = 280.0;
const PT_TO_MM: f32 = 25.4 / 72.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Sans,
    SansMedium,
    SansSemibold,
    Mono,
    MonoMedium,
    MonoSemibold,
}

impl Face {
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Sans => include_bytes!("../../assets/fonts/NotoSans-Regular.ttf"),
            Self::SansMedium => include_bytes!("../../assets/fonts/NotoSans-Medium.ttf"),
            Self::SansSemibold => include_bytes!("../../assets/fonts/NotoSans-SemiBold.ttf"),
            Self::Mono => include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
            Self::MonoMedium => include_bytes!("../../assets/fonts/JetBrainsMono-Medium.ttf"),
            Self::MonoSemibold => include_bytes!("../../assets/fonts/JetBrainsMono-SemiBold.ttf"),
        }
    }

    fn family(self) -> FontFamily {
        FontFamily::Name(format!("document-{self:?}").into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub face: Face,
    /// Physical typographic points (1/72 inch), independent of preview zoom.
    pub size_pt: f32,
    /// Baseline-to-baseline distance in millimetres.
    pub leading_mm: f32,
}

impl TextStyle {
    pub const BODY: Self = Self {
        face: Face::Sans,
        size_pt: 9.0,
        leading_mm: 4.8,
    };
    pub const HEADING: Self = Self {
        face: Face::SansSemibold,
        size_pt: 13.0,
        leading_mm: 7.0,
    };
    pub const CAPTION: Self = Self {
        face: Face::SansMedium,
        size_pt: 8.0,
        leading_mm: 4.3,
    };
    pub const MONO: Self = Self {
        face: Face::Mono,
        size_pt: 8.0,
        leading_mm: 4.4,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub character: char,
    /// Baseline origin of this character in page millimetres.
    pub baseline: Point,
    pub advance_mm: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: TextStyle,
    pub glyphs: Vec<PositionedGlyph>,
    /// Logical bounds, including advances/line height; use for clipping checks.
    pub bounds: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ink {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Ink {
    pub const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub ink: Ink,
    pub width_mm: f32,
}

impl Stroke {
    pub const STANDARD: Self = Self {
        ink: Ink::BLACK,
        width_mm: 0.25,
    };
}

#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Text(TextRun),
    Path {
        points: Vec<Point>,
        closed: bool,
        stroke: Stroke,
    },
    Box {
        bounds: Rect,
        stroke: Option<Stroke>,
        fill: Option<Ink>,
    },
    /// Clickable annotation bounds and destination resolved in both renderers.
    Link {
        bounds: Rect,
        destination: String,
    },
    /// A notice has positioned text like any other content, but its role cannot
    /// be lost when optional sections are filtered by the packet builder.
    Notice(TextRun),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub number: usize,
    pub width_mm: f32,
    pub height_mm: f32,
    pub primitives: Vec<Primitive>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub pages: Vec<Page>,
    pub layout_version: u32,
    pub font_metrics_version: u32,
}

/// All strings here are already localized and belong to the immutable prepared snapshot.
#[derive(Clone, Debug)]
pub struct PageContext {
    pub project: String,
    pub revision: String,
    pub packet: String,
    /// Stamp on every page, e.g. "DRAFT / NOT FOR CUTTING".
    pub stamp: Option<String>,
    /// Safety/scale disclosure repeated in the footer of every page.
    pub safety: String,
    /// Mandatory first-page issues/disclosures, retained even if all details are off.
    pub notices: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidGeometry,
    MissingGlyph(char),
    BlockTooTall,
}

/// Uses egui's own harfrust-backed layout on the same bundled faces as the UI.
/// Its fixed 1 px/point pass makes pagination independent of display scale.
pub struct FontMetrics {
    context: egui::Context,
}

impl FontMetrics {
    pub fn new() -> Self {
        let context = egui::Context::default();
        let mut fonts = FontDefinitions::default();
        let fallback = fonts.families[&FontFamily::Proportional].clone();
        for face in [
            Face::Sans,
            Face::SansMedium,
            Face::SansSemibold,
            Face::Mono,
            Face::MonoMedium,
            Face::MonoSemibold,
        ] {
            fonts.font_data.insert(
                format!("document-{face:?}"),
                FontData::from_static(face.bytes()).into(),
            );
            let mut family = vec![format!("document-{face:?}")];
            family.extend(fallback.iter().cloned());
            fonts.families.insert(face.family(), family);
        }
        context.set_fonts(fonts);
        // Font atlas / galley state requires a pass; never tied to a native window.
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        Self { context }
    }

    fn shape(
        &self,
        text: &str,
        style: TextStyle,
    ) -> Result<Vec<(char, f32, f32, f32)>, LayoutError> {
        if !style.size_pt.is_finite()
            || style.size_pt <= 0.0
            || !style.leading_mm.is_finite()
            || style.leading_mm <= 0.0
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let id = FontId::new(style.size_pt, style.face.family());
        self.context.fonts_mut(|fonts| {
            for ch in text.chars() {
                if !fonts.has_glyph(&id, ch) {
                    return Err(LayoutError::MissingGlyph(ch));
                }
            }
            let galley = fonts.layout_no_wrap(text.to_owned(), id, Color32::BLACK);
            Ok(galley
                .rows
                .iter()
                .flat_map(|row| {
                    row.glyphs.iter().map(move |glyph| {
                        (
                            glyph.chr,
                            (row.pos.x + glyph.pos.x) * PT_TO_MM,
                            (row.pos.y + glyph.pos.y) * PT_TO_MM,
                            glyph.advance_width * PT_TO_MM,
                        )
                    })
                })
                .collect())
        })
    }

    pub fn width_mm(&self, text: &str, style: TextStyle) -> Result<f32, LayoutError> {
        let glyphs = self.shape(text, style)?;
        Ok(glyphs.last().map_or(0.0, |(_, x, _, w)| x + w))
    }

    /// Wrap at whitespace, falling back to scalar boundaries for an oversized
    /// word/UUID. Never elide or discard the characters of a long identifier.
    pub fn wrap(
        &self,
        text: &str,
        style: TextStyle,
        width_mm: f32,
    ) -> Result<Vec<String>, LayoutError> {
        if !width_mm.is_finite() || width_mm <= 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_owned()
                } else {
                    format!("{line} {word}")
                };
                if self.width_mm(&candidate, style)? <= width_mm {
                    line = candidate;
                    continue;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                for ch in word.chars() {
                    let candidate = format!("{line}{ch}");
                    if self.width_mm(&candidate, style)? > width_mm && !line.is_empty() {
                        lines.push(std::mem::take(&mut line));
                    }
                    line.push(ch);
                    if self.width_mm(&line, style)? > width_mm {
                        return Err(LayoutError::BlockTooTall);
                    }
                }
            }
            lines.push(std::mem::take(&mut line));
        }
        Ok(lines)
    }

    fn run(
        &self,
        text: String,
        style: TextStyle,
        x: f32,
        top: f32,
        width: f32,
    ) -> Result<TextRun, LayoutError> {
        let mut shaped = self.shape(&text, style)?;
        // egui preserves a one-entry-per-character contract for ligatures by
        // appending zero-width continuation glyphs. Both page renderers draw
        // individual Unicode scalars, so retaining a continuation's origin
        // would print it on top of the following character. Divide the
        // ligature's *existing* advance according to the component glyphs'
        // natural advances. This leaves run width, wrapping and page breaks
        // exactly as measured by the original full-string shaper.
        let mut index = 0;
        while index < shaped.len() {
            let mut end = index + 1;
            while end < shaped.len() && shaped[end].3 == 0.0 {
                end += 1;
            }
            if end > index + 1 {
                let widths = shaped[index..end]
                    .iter()
                    .map(|(ch, _, _, _)| {
                        self.shape(&ch.to_string(), style)
                            .map(|glyphs| glyphs.first().map_or(0.0, |g| g.3))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let total = widths.iter().sum::<f32>();
                let original_width = shaped[index].3;
                if total <= 0.0 || original_width <= 0.0 {
                    return Err(LayoutError::InvalidGeometry);
                }
                let mut offset = 0.0;
                let start = shaped[index].1;
                for (glyph, natural_width) in shaped[index..end].iter_mut().zip(widths) {
                    glyph.1 = start + offset;
                    glyph.3 = original_width * natural_width / total;
                    offset += glyph.3;
                }
            }
            index = end;
        }
        let actual_width = shaped
            .last()
            .map_or(0.0, |(_, offset, _, advance)| offset + advance);
        if actual_width > width + 0.001 {
            return Err(LayoutError::InvalidGeometry);
        }
        let glyphs = shaped
            .into_iter()
            .map(
                |(character, offset, baseline, advance_mm)| PositionedGlyph {
                    character,
                    baseline: Point {
                        x: x + offset,
                        y: top + baseline,
                    },
                    advance_mm,
                },
            )
            .collect();
        Ok(TextRun {
            text,
            style,
            glyphs,
            bounds: Rect {
                x,
                y: top,
                width: actual_width,
                height: style.leading_mm,
            },
        })
    }
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Flows paragraphs and table rows into fixed pages. Repeats project/revision,
/// draft mark, current section/table column headings and safety footer on breaks.
pub struct DocumentBuilder {
    metrics: FontMetrics,
    context: PageContext,
    document: Document,
    cursor: f32,
    bottom: f32,
    repeating: bool,
    section: Option<String>,
    table_header: Option<Vec<String>>,
    continuation: Vec<String>,
}

impl DocumentBuilder {
    pub fn new(context: PageContext) -> Result<Self, LayoutError> {
        if context.project.trim().is_empty()
            || context.revision.trim().is_empty()
            || context.packet.trim().is_empty()
            || context.safety.trim().is_empty()
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let mut builder = Self {
            metrics: FontMetrics::new(),
            context,
            document: Document {
                pages: Vec::new(),
                layout_version: LAYOUT_VERSION,
                font_metrics_version: FONT_METRICS_VERSION,
            },
            cursor: TOP_MM,
            bottom: BOTTOM_MM,
            repeating: false,
            section: None,
            table_header: None,
            continuation: Vec::new(),
        };
        builder.new_page()?;
        for notice in builder.context.notices.clone() {
            builder.notice(&notice)?;
        }
        Ok(builder)
    }

    fn page(&mut self) -> &mut Page {
        self.document.pages.last_mut().expect("page initialized")
    }

    fn fixed_text(
        &mut self,
        text: &str,
        style: TextStyle,
        x: f32,
        y: f32,
        width: f32,
        notice: bool,
    ) -> Result<f32, LayoutError> {
        let mut bottom = y;
        for line in self.metrics.wrap(text, style, width)? {
            if bottom + style.leading_mm > A4_HEIGHT_MM - 4.0 {
                return Err(LayoutError::BlockTooTall);
            }
            let run = self.metrics.run(line, style, x, bottom, width)?;
            self.page().primitives.push(if notice {
                Primitive::Notice(run)
            } else {
                Primitive::Text(run)
            });
            bottom += style.leading_mm;
        }
        Ok(bottom)
    }

    fn new_page(&mut self) -> Result<(), LayoutError> {
        self.repeating = true;
        self.document.pages.push(Page {
            number: self.document.pages.len() + 1,
            width_mm: A4_WIDTH_MM,
            height_mm: A4_HEIGHT_MM,
            primitives: Vec::new(),
        });
        let width = A4_WIDTH_MM - 2.0 * MARGIN_MM;
        let mut y = 7.0;
        if let Some(stamp) = self.context.stamp.clone() {
            y = self.fixed_text(&stamp, TextStyle::CAPTION, MARGIN_MM, y, width, true)?;
        }
        let header = format!(
            "{} · {} · {}",
            self.context.project, self.context.revision, self.context.packet
        );
        y = self.fixed_text(&header, TextStyle::CAPTION, MARGIN_MM, y, width, false)?;
        self.cursor = y.max(TOP_MM) + 2.0;
        let safety = self.context.safety.clone();
        let footer_lines = self.metrics.wrap(&safety, TextStyle::CAPTION, width)?.len();
        let footer_y = A4_HEIGHT_MM - 4.0 - footer_lines as f32 * TextStyle::CAPTION.leading_mm;
        let footer_end = self.fixed_text(
            &safety,
            TextStyle::CAPTION,
            MARGIN_MM,
            footer_y,
            width,
            true,
        )?;
        if footer_end > A4_HEIGHT_MM - 4.0 {
            return Err(LayoutError::BlockTooTall);
        }
        self.bottom = footer_y - 3.0;
        if let Some(section) = self.section.clone() {
            self.write_lines(&section, TextStyle::HEADING, true)?;
        }
        if let Some(columns) = self.table_header.clone() {
            self.table_cells(&columns, true)?;
        }
        for line in self.continuation.clone() {
            self.write_lines(&line, TextStyle::CAPTION, false)?;
        }
        self.repeating = false;
        if self.cursor + TextStyle::BODY.leading_mm > self.bottom {
            return Err(LayoutError::BlockTooTall);
        }
        Ok(())
    }

    fn write_lines(
        &mut self,
        text: &str,
        style: TextStyle,
        notice: bool,
    ) -> Result<(), LayoutError> {
        for line in self
            .metrics
            .wrap(text, style, A4_WIDTH_MM - 2.0 * MARGIN_MM)?
        {
            self.reserve(style.leading_mm)?;
            let run = self.metrics.run(
                line,
                style,
                MARGIN_MM,
                self.cursor,
                A4_WIDTH_MM - 2.0 * MARGIN_MM,
            )?;
            self.page().primitives.push(if notice {
                Primitive::Notice(run)
            } else {
                Primitive::Text(run)
            });
            self.cursor += style.leading_mm;
        }
        Ok(())
    }

    fn reserve(&mut self, height: f32) -> Result<(), LayoutError> {
        if !height.is_finite() || height <= 0.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        if height > BOTTOM_MM - TOP_MM - 30.0 {
            return Err(LayoutError::BlockTooTall);
        }
        if self.cursor + height > self.bottom {
            if self.repeating {
                return Err(LayoutError::BlockTooTall);
            }
            self.new_page()?;
        }
        if self.cursor + height > self.bottom {
            return Err(LayoutError::BlockTooTall);
        }
        Ok(())
    }

    pub fn paragraph(&mut self, text: &str) -> Result<(), LayoutError> {
        self.write_lines(text, TextStyle::BODY, false)
    }
    pub fn notice(&mut self, text: &str) -> Result<(), LayoutError> {
        self.write_lines(text, TextStyle::BODY, true)
    }

    pub fn section(&mut self, name: &str) -> Result<(), LayoutError> {
        self.section = None;
        self.table_header = None;
        self.continuation.clear();
        self.reserve(2.0 * TextStyle::HEADING.leading_mm)?;
        self.cursor += 3.0;
        self.write_lines(name, TextStyle::HEADING, false)?;
        self.section = Some(name.to_owned());
        Ok(())
    }

    /// Start a sheet on a fresh page; the project, stamp and safety footer are
    /// installed by the same page path as ordinary overflow.
    pub fn page_break(&mut self) -> Result<(), LayoutError> {
        self.new_page()
    }

    /// Repeat short diagram/sequence metadata on every continuation page until
    /// the next section. Callers print the initial copy separately.
    pub fn set_continuation(&mut self, lines: Vec<String>) {
        self.continuation = lines;
    }

    /// Equal-width columns, independently wrapped; one row stays together.
    /// A row too tall for a single page is rejected rather than silently clipped.
    pub fn table(&mut self, headings: &[String], rows: &[Vec<String>]) -> Result<(), LayoutError> {
        if headings.is_empty() || rows.iter().any(|row| row.len() != headings.len()) {
            return Err(LayoutError::InvalidGeometry);
        }
        self.table_header = None;
        self.table_cells(headings, true)?;
        self.table_header = Some(headings.to_vec());
        for row in rows {
            self.table_cells(row, false)?;
        }
        self.table_header = None;
        Ok(())
    }

    fn table_cells(&mut self, cells: &[String], heading: bool) -> Result<(), LayoutError> {
        let style = if heading {
            TextStyle::CAPTION
        } else {
            TextStyle::BODY
        };
        let column_width = (A4_WIDTH_MM - 2.0 * MARGIN_MM) / cells.len() as f32;
        let lines: Vec<_> = cells
            .iter()
            .map(|text| self.metrics.wrap(text, style, column_width - 3.0))
            .collect::<Result<_, _>>()?;
        let height = lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * style.leading_mm + 2.0;
        self.reserve(height)?;
        for (col, column_lines) in lines.into_iter().enumerate() {
            for (row, line) in column_lines.into_iter().enumerate() {
                let run = self.metrics.run(
                    line,
                    style,
                    MARGIN_MM + col as f32 * column_width,
                    self.cursor + row as f32 * style.leading_mm,
                    column_width - 3.0,
                )?;
                self.page().primitives.push(Primitive::Text(run));
            }
        }
        self.cursor += height;
        Ok(())
    }

    /// Reserve a known-size diagram. Later stages place witness geometry within
    /// this box using top-left physical millimetres, with no renderer scaling.
    pub fn diagram(&mut self, height_mm: f32) -> Result<Rect, LayoutError> {
        self.reserve(height_mm)?;
        let rect = Rect {
            x: MARGIN_MM,
            y: self.cursor,
            width: A4_WIDTH_MM - 2.0 * MARGIN_MM,
            height: height_mm,
        };
        self.page().primitives.push(Primitive::Box {
            bounds: rect,
            stroke: Some(Stroke::STANDARD),
            fill: None,
        });
        self.cursor += height_mm;
        Ok(rect)
    }

    /// Draw a clipped-to-paper region (parts and actual blade bands), retaining
    /// physical measurements rather than asking a renderer to interpret them.
    pub fn box_at(
        &mut self,
        bounds: Rect,
        stroke: Option<Stroke>,
        fill: Option<Ink>,
    ) -> Result<(), LayoutError> {
        if !bounds.x.is_finite()
            || !bounds.y.is_finite()
            || !bounds.width.is_finite()
            || !bounds.height.is_finite()
            || bounds.x < MARGIN_MM
            || bounds.y < 0.0
            || bounds.width <= 0.0
            || bounds.height <= 0.0
            || bounds.x + bounds.width > A4_WIDTH_MM - MARGIN_MM
            || bounds.y + bounds.height > self.bottom
        {
            return Err(LayoutError::InvalidGeometry);
        }
        self.page().primitives.push(Primitive::Box {
            bounds,
            stroke,
            fill,
        });
        Ok(())
    }

    pub fn measure(&self, text: &str, style: TextStyle) -> Result<f32, LayoutError> {
        self.metrics.width_mm(text, style)
    }

    /// Place a witness-derived line/polyline on the current page.
    /// Geometry is validated against the paper before it enters the page model.
    pub fn path(&mut self, points: Vec<Point>, closed: bool) -> Result<(), LayoutError> {
        self.path_styled(points, closed, Stroke::STANDARD)
    }

    pub fn path_styled(
        &mut self,
        points: Vec<Point>,
        closed: bool,
        stroke: Stroke,
    ) -> Result<(), LayoutError> {
        if points.len() < 2
            || !stroke.width_mm.is_finite()
            || stroke.width_mm <= 0.0
            || points.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x < MARGIN_MM
                    || p.x > A4_WIDTH_MM - MARGIN_MM
                    || p.y < 0.0
                    || p.y > self.bottom
            })
        {
            return Err(LayoutError::InvalidGeometry);
        }
        self.page().primitives.push(Primitive::Path {
            points,
            closed,
            stroke,
        });
        Ok(())
    }

    /// Position a short diagram annotation using the same shaped glyph origins
    /// and font bytes as flowed text. No backend is allowed to re-wrap it.
    pub fn label_at(
        &mut self,
        text: &str,
        style: TextStyle,
        x: f32,
        y: f32,
        width: f32,
    ) -> Result<(), LayoutError> {
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || x < MARGIN_MM
            || x + width > A4_WIDTH_MM - MARGIN_MM
            || y < 0.0
            || y + style.leading_mm > self.bottom
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let run = self.metrics.run(text.to_owned(), style, x, y, width)?;
        self.page().primitives.push(Primitive::Text(run));
        Ok(())
    }

    pub fn link(&mut self, bounds: Rect, destination: String) -> Result<(), LayoutError> {
        if destination.is_empty()
            || !bounds.x.is_finite()
            || !bounds.y.is_finite()
            || !bounds.width.is_finite()
            || !bounds.height.is_finite()
            || bounds.x < MARGIN_MM
            || bounds.y < 0.0
            || bounds.width <= 0.0
            || bounds.height <= 0.0
            || bounds.x + bounds.width > A4_WIDTH_MM - MARGIN_MM
            || bounds.y + bounds.height > self.bottom
        {
            return Err(LayoutError::InvalidGeometry);
        }
        self.page().primitives.push(Primitive::Link {
            bounds,
            destination,
        });
        Ok(())
    }

    pub fn finish(self) -> Document {
        self.document
    }
}
