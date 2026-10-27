//! The document model VectorCraft imports: spreads, artboards, layers, groups, curves, shapes,
//! text and images, with transforms composed into document space. Anything else is reported
//! by [`Document::warnings`] instead of being dropped silently.

//! Adapted from VectorCraft crates/affinity/src/model.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, with AF.5 records from
//! b3ccdf01a7482fc4fd7545ed9e302911824a2a3c. MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.

use std::collections::{BTreeMap, HashSet};

use crate::paint::{self, Paint, Stroke};
use crate::stream::{ObjId, Stream, Tag};
use crate::{Archive, Error, Limits, stream};

/// Affine map `x' = a·x + c·y + e`, `y' = b·x + d·y + f` (kurbo's coefficient order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Affine {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Affinity's `Xfrm`, stored row by row: (a, c, e, b, d, f).
    pub(crate) fn from_xfrm(m: [f64; 6]) -> Self {
        Self([m[0], m[3], m[1], m[4], m[2], m[5]])
    }

    pub fn then(self, outer: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [oa, ob, oc, od, oe, of] = outer.0;
        Self([oa * a + oc * b, ob * a + od * b, oa * c + oc * d, ob * c + od * d, oa * e + oc * f + oe, ob * e + od * f + of])
    }

    pub fn apply(&self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point { x: a * p.x + c * p.y + e, y: b * p.x + d * p.y + f }
    }

    /// Geometric mean of the axis scales: how much a stroke width or font size grows.
    pub fn scale(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }

    pub fn is_finite(&self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// One cubic Bézier subpath, in document pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct SubPath {
    pub start: Point,
    /// (control 1, control 2, end) per segment.
    pub segments: Vec<[Point; 3]>,
    pub closed: bool,
}

/// An outline in document pixels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub subpaths: Vec<SubPath>,
}

impl Path {
    pub fn transformed(&self, m: Affine) -> Self {
        Self {
            subpaths: self
                .subpaths
                .iter()
                .map(|s| SubPath { start: m.apply(s.start), segments: s.segments.iter().map(|seg| seg.map(|p| m.apply(p))).collect(), closed: s.closed })
                .collect(),
        }
    }
}

/// Affinity's blend modes, numbered by the `Blnd` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    #[default]
    Normal,
    PassThrough,
    Darken,
    DarkerColor,
    Multiply,
    ColorBurn,
    Lighten,
    LighterColor,
    Screen,
    ColorDodge,
    Add,
    Overlay,
    SoftLight,
    HardLight,
    VividLight,
    PinLight,
    LinearLight,
    HardMix,
    Difference,
    Exclusion,
    Subtract,
    Hue,
    Saturation,
    Luminosity,
    Color,
    Average,
    Negation,
    Reflect,
    Glow,
    Erase,
}

impl Blend {
    /// The enum (id, version) pairs observed in a document whose layers are named after their mode.
    fn from_enum(id: u16, version: u16) -> Option<Self> {
        use Blend::*;
        Some(match (id, version) {
            (0, _) => Normal,
            (1, 0) => Darken,
            (2, 1) => DarkerColor,
            (2, 0) => Multiply,
            (3, 0) => ColorBurn,
            (4, 0) => Lighten,
            (6, 1) => LighterColor,
            (5, 0) => Screen,
            (6, 0) => ColorDodge,
            (7, 0) => Add,
            (8, 0) => Overlay,
            (9, 0) => SoftLight,
            (10, 0) => HardLight,
            (11, 0) => VividLight,
            (12, 0) => PinLight,
            (15, 1) => LinearLight,
            (13, 0) => HardMix,
            (14, 0) => Difference,
            (15, 0) => Exclusion,
            (16, 0) => Subtract,
            (17, 0) => Hue,
            (18, 0) => Saturation,
            (19, 0) => Luminosity,
            (20, 0) => Color,
            (21, 0) => Average,
            (22, 0) => Negation,
            (23, 0) => Reflect,
            (24, 0) => Glow,
            (25, 0) => Erase,
            _ => return None,
        })
    }
}

/// A run of text with one set of character attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// PostScript name and family of the chosen font.
    pub postscript: String,
    pub family: String,
    /// CSS weight (400 regular, 700 bold).
    pub weight: i64,
    pub italic: bool,
    /// Boolean OpenType feature overrides, as tags or `-tag` for disabled features.
    pub features: Vec<String>,
    pub all_caps: bool,
    /// Underlined characters (`Ints` slot 0 is 1; identified on underlined words in a document its
    /// owner made, all other characters there and in the public corpus have 0).
    pub underline: bool,
    /// Struck-through characters (`Ints` slot 1 is 1; identified the same way, on prices struck
    /// through in another document its owner made).
    pub strikethrough: bool,
    /// Font size in document pixels, before the node's transform.
    pub size: f64,
    /// Tracking in em.
    pub tracking: f64,
    /// Horizontal scale of the glyphs (1 = as designed).
    pub h_scale: f64,
    /// Fixed line pitch in document pixels, when the run overrides automatic leading.
    pub leading: Option<f64>,
    pub fill: Paint,
    /// Outlined characters: the outline's paint and width (node space).
    pub stroke: Option<crate::paint::Stroke>,
    /// The run's paragraph: alignment, indents and spacing.
    pub paragraph: Paragraph,
}

/// Paragraph alignment, indents, spacing and leading, in node space (document pixels before the
/// node's text scale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paragraph {
    pub align: Align,
    /// Indent of every line but the first, from the left.
    pub left: f64,
    pub right: f64,
    /// Indent of the first line, from the left.
    pub first: f64,
    pub space_before: f64,
    pub space_after: f64,
    /// Fixed distance between baselines, when the paragraph sets one.
    pub leading: Option<f64>,
    /// Automatic leading as a multiple of the font's own line height (ascent plus descent).
    pub auto_leading: f64,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self { align: Align::Left, left: 0.0, right: 0.0, first: 0.0, space_before: 0.0, space_after: 0.0, leading: None, auto_leading: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub runs: Vec<TextRun>,
    pub align: Align,
    /// Artistic text: the first baseline's anchor point in node space (start, centre or end of the
    /// line according to `align`). Frame text: the frame box in node space.
    pub anchor: Point,
    pub frame: Option<Rect>,
    /// Node space to document pixels.
    pub transform: Affine,
    /// Node space to document pixels for the type (its linear part matters): artistic text scales
    /// with its node, frame text only with its frame's text scale.
    pub text_transform: Affine,
}

/// Vertical alignment of a table cell's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub runs: Vec<TextRun>,
    /// Space between the cell edges and its text, in node space: left, top, right, bottom.
    pub inset: [f64; 4],
    pub valign: VAlign,
    /// Part of the cell to its left (merged): it has no text of its own.
    pub merged_left: bool,
}

/// A table: a grid of cells with text, and a line on every cell edge.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Column boundaries (one more than the columns), increasing, in node space.
    pub columns: Vec<f64>,
    /// Row boundaries (one more than the rows), increasing, in node space.
    pub rows: Vec<f64>,
    /// Row by row.
    pub cells: Vec<TableCell>,
    /// Vertical edges row by row, one more per row than the columns; `None` draws no line, and
    /// the list is empty when the file's didn't match the grid.
    pub vertical: Vec<Option<Stroke>>,
    /// Horizontal edges boundary by boundary (one more than the rows), one per column (empty
    /// as `vertical` can be).
    pub horizontal: Vec<Option<Stroke>>,
    /// Node space to document pixels.
    pub transform: Affine,
    /// Node space to document pixels for the cells' type: resizing a table moves its grid, not
    /// its type.
    pub text_transform: Affine,
}

impl Table {
    pub fn ncols(&self) -> usize {
        self.columns.len().saturating_sub(1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pixels {
    /// Straight-alpha RGBA, 8 bits per channel, row by row.
    Rgba8(Vec<u8>),
    /// A CMYK pixel layer: cyan, magenta, yellow and black ink amounts (0 none, 255 full) and
    /// straight alpha, 8 bits each, row by row. Left in CMYK for the importer to show through a
    /// colour profile: the profile-free formula turns the extra cyan of a separation's neutrals
    /// into a blue cast.
    Cmyka8(Vec<u8>),
    /// The original file a placed image was made from (PNG, JPEG…), as embedded.
    Encoded(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Embedded source profile; shared by images from the same native document.
    pub profile: Option<std::sync::Arc<Vec<u8>>>,
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
    /// Pixel space (0..w, 0..h) to document pixels.
    pub transform: Affine,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Layer container (`Scop`).
    Layer,
    Group,
    /// Artboard: an export rectangle and vector outline that clips its children.
    /// `background` paints behind them, and `strokes` over them.
    Artboard {
        rect: Rect,
        /// Actual clipping outline in document pixels. The artboard's export rectangle is its
        /// bounding box, but a rotated or non-rectangular board still clips to this outline.
        path: Path,
        even_odd: bool,
        background: Vec<Paint>,
        strokes: Vec<Stroke>,
    },
    /// Curve, parametric shape or compound shape: an outline with its fills and strokes.
    /// Children of a shape are clipped by its outline (painted over its fills, under its strokes).
    Shape {
        path: Path,
        fills: Vec<Paint>,
        strokes: Vec<Stroke>,
        even_odd: bool,
    },
    Text(Text),
    Table(Table),
    Image(Image),
    /// A master page shown on this spread's pages: pages `page..page + count` show the master
    /// spread [`Document::masters`]`[master]`'s pages from `master_page` on.
    MasterInstance {
        master: usize,
        page: usize,
        master_page: usize,
        count: usize,
    },
    /// Something this reader does not import; its children are still imported.
    Unsupported,
}

/// A layer effect. Lengths are document pixels; `angle` is the direction the shadow is offset in
/// (radians, clockwise from +x as y points down: π/2 puts it below).
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    DropShadow {
        color: crate::paint::Color,
        opacity: f64,
        radius: f64,
        distance: f64,
        angle: f64,
    },
    InnerShadow {
        color: crate::paint::Color,
        opacity: f64,
        radius: f64,
        distance: f64,
        angle: f64,
    },
    OuterGlow {
        color: crate::paint::Color,
        opacity: f64,
        radius: f64,
    },
    /// An outline of `width` around the node's shape: `align` 0 outside, 1 inside, 2 centred.
    Outline {
        color: crate::paint::Color,
        opacity: f64,
        width: f64,
        align: u16,
    },
    /// Bevel and emboss: lit and shaded edges `size` wide. `angle` is the light's azimuth
    /// (radians, counter-clockwise from +x with y up), `depth` Affinity's depth (5 by default);
    /// `inner` is false for the outer, emboss and pillow kinds.
    Bevel {
        size: f64,
        depth: f64,
        angle: f64,
        highlight: crate::paint::Color,
        highlight_opacity: f64,
        shadow: crate::paint::Color,
        shadow_opacity: f64,
        inner: bool,
    },
    /// Colour overlay: everything the node paints takes `color`, laid over it in `blend` mode at
    /// `opacity` (the node's own alpha is kept).
    ColorOverlay {
        color: crate::paint::Color,
        opacity: f64,
        blend: Blend,
    },
    /// Gradient overlay: a colour overlay whose colour comes from `gradient` (document pixels).
    GradientOverlay {
        gradient: crate::paint::Gradient,
        opacity: f64,
        blend: Blend,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub class: Tag,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f64,
    pub blend: Blend,
    pub kind: Kind,
    /// Vector mask in document pixels: the node shows only inside it (nonzero fill).
    pub mask: Option<Path>,
    /// Pixel mask: grey levels (white shows the node, black hides it); outside it the node is hidden.
    pub pixel_mask: Option<Image>,
    /// Layer effects the importer can show or bake in (others are reported).
    pub effects: Vec<Effect>,
    /// Bottom to top.
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spread {
    /// Spread bounds in document pixels (the canvas, or the union of the artboards).
    pub bounds: Rect,
    /// Pages of a Publisher-style spread (one, or two facing pages), in document pixels.
    pub pages: Vec<Rect>,
    /// Transparent background; otherwise the page is white.
    pub transparent: bool,
    /// Bottom to top.
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Pixels per inch: all geometry is in document pixels at this resolution.
    pub dpi: f64,
    pub spreads: Vec<Spread>,
    /// Master spreads, which [`Kind::MasterInstance`] nodes show on pages.
    pub masters: Vec<Spread>,
    /// Application that last saved the file, as stored (`Affinity 3.0.2`), when present.
    pub saved_by: Option<String>,
    /// Content that was not imported or was approximated, one line per kind, deduplicated.
    pub warnings: Vec<String>,
    /// The document's CMYK colour profile (an ICC file), when it embeds one: the root's `CMYP`,
    /// which each CMYK pixel layer's colour settings (`Prof`) also name, with relative
    /// colorimetric intent and black-point compensation in every document checked.
    pub cmyk_profile: Option<std::sync::Arc<Vec<u8>>>,
}

/// Largest embedded colour profile kept (real CMYK profiles are a few megabytes).
const MAX_PROFILE_BYTES: usize = 32 << 20;

/// Deepest layer nesting followed; deeper content is reported, not imported.
const MAX_TREE_DEPTH: usize = 128;
/// Total layer nodes imported.
const MAX_NODES: usize = 500_000;
const MAX_CHILD_SLOTS: usize = 8_000_000;
const MAX_NAME_BYTES: usize = 64 << 20;
const MAX_DOCUMENT_CHARS: usize = 16 << 20;
const MAX_DOCUMENT_CELLS: usize = 1 << 18;
const MAX_EMBED_DEPTH: usize = 4;

/// Read a document: archive, `doc.dat` and the model.
pub fn read(bytes: &[u8], limits: Limits) -> Result<Document, Error> {
    read_with(bytes, limits, None)
}
pub fn read_with<'a>(bytes: &'a [u8], limits: Limits, cancel: Option<&'a std::sync::atomic::AtomicBool>) -> Result<Document, Error> {
    let mut archive = Archive::open_with(bytes, limits, cancel)?;
    let doc = archive.read("doc.dat")?;
    let s = stream::parse_with(&doc, limits, cancel)?;
    Reader::new(&s, &mut archive).document()
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Budget {
    nodes: usize,
    depth: usize,
    pixels: u64,
    child_slots: usize,
    name_bytes: usize,
    pub(crate) owned: usize,
    pub(crate) chars: usize,
    pub(crate) cells: usize,
    limit: usize,
}

pub(crate) struct Reader<'s, 'a, 'b> {
    pub(crate) s: &'s Stream,
    pub(crate) archive: &'b mut Archive<'a>,
    pub(crate) dpi: f64,
    pub(crate) stream_depth: usize,
    warnings: BTreeMap<String, usize>,
    pub(crate) budget: Budget,
    base: Affine,
    pub(crate) type_base: Affine,
    embed_depth: usize,
    archive_base: usize,
    pending: Option<Vec<Node>>,
    master_ids: BTreeMap<ObjId, usize>,
    active: HashSet<ObjId>,
    pub(crate) segments: usize,
    pub(crate) failure: Option<Error>,
}

impl<'s, 'a, 'b> Reader<'s, 'a, 'b> {
    pub(crate) fn new(s: &'s Stream, archive: &'b mut Archive<'a>) -> Self {
        let limit = archive.limits.max_total;
        let stream_depth = archive.limits.max_depth;
        Self {
            s,
            archive,
            dpi: 72.0,
            stream_depth,
            warnings: BTreeMap::new(),
            budget: Budget { limit, chars: MAX_DOCUMENT_CHARS, cells: MAX_DOCUMENT_CELLS, ..Default::default() },
            base: Affine::IDENTITY,
            type_base: Affine::IDENTITY,
            embed_depth: 0,
            archive_base: 0,
            pending: None,
            master_ids: BTreeMap::new(),
            active: HashSet::new(),
            segments: 0,
            failure: None,
        }
    }

    /// Shared arrays are scanned per occurrence, including slots that produce no nodes.
    pub(crate) fn charge_slots(&mut self, count: usize) -> bool {
        if let Err(error) = crate::import::cancelled(self.archive.cancel) {
            self.failure = Some(error);
            return false;
        }
        self.budget.child_slots = self.budget.child_slots.saturating_add(count);
        if self.budget.child_slots > MAX_CHILD_SLOTS {
            self.failure = Some(Error::Limit("array traversal slots"));
            return false;
        }
        true
    }

    pub(crate) fn charge_owned(&mut self, bytes: usize) -> bool {
        if let Err(e) = crate::import::cancelled(self.archive.cancel) {
            self.failure = Some(e);
            return false;
        }
        self.budget.owned = self.budget.owned.saturating_add(bytes);
        let remaining =
            self.budget.limit.checked_sub(self.budget.owned).and_then(|v| v.checked_sub(self.archive_base)).filter(|v| *v >= self.archive.extracted());
        let Some(remaining) = remaining else {
            self.failure = Some(Error::Limit("expanded model allocation"));
            return false;
        };
        self.archive.limits.max_total = remaining;
        true
    }
    pub(crate) fn pixel_capacity(&self) -> u64 {
        let bytes = self.budget.limit.saturating_sub(self.budget.owned).saturating_sub(self.archive_base).saturating_sub(self.archive.extracted());
        self.archive.limits.max_pixels.saturating_sub(self.budget.pixels).min(bytes as u64 / 16)
    }
    pub(crate) fn charge_pixels(&mut self, count: u64) -> Result<(), Error> {
        if !self.charge_owned(usize::try_from(count.saturating_mul(16)).unwrap_or(usize::MAX)) {
            return Err(self.failure.unwrap_or(Error::Limit("pixel allocation")));
        }
        self.budget.pixels =
            self.budget.pixels.checked_add(count).filter(|v| *v <= self.archive.limits.max_pixels).ok_or(Error::Limit("decoded pixel count"))?;
        crate::import::cancelled(self.archive.cancel)
    }
    pub(crate) fn warn(&mut self, what: impl Into<String>) {
        *self.warnings.entry(what.into()).or_default() += 1;
    }

    pub(crate) fn document(&mut self) -> Result<Document, Error> {
        let s = self.s;
        let root = s.root;
        if let Some(units) = s.obj(root, b"UVCn") {
            s.check_finite(units, b"UPPI")?;
        }
        self.dpi = s.obj(root, b"UVCn").and_then(|u| s.f64(u, b"UPPI")).unwrap_or(72.0);
        if !self.dpi.is_finite() || !(1.0..=100_000.0).contains(&self.dpi) {
            return Err(Error::Malformed("document DPI"));
        }
        let saved_by = s.obj(root, b"NVer").map(|v| {
            let product = s.str(v, b"Prod").unwrap_or("Affinity");
            match (s.int(v, b"Majr"), s.int(v, b"Minr"), s.int(v, b"Bild")) {
                (Some(a), Some(b), Some(c)) => format!("{product} {a}.{b}.{c}"),
                _ => product.to_string(),
            }
        });
        let doc = s.obj(root, b"DocR").ok_or(Error::Malformed("document has no root node"))?;
        let master_count = s.array_len(doc, b"MpCh");
        if !self.charge_slots(s.array_len(doc, b"Chld").saturating_add(master_count)) || !self.charge_owned(master_count.saturating_mul(64)) {
            return Err(self.failure.unwrap_or(Error::Limit("spread slots")));
        }
        let masters = s.objs(doc, b"MpCh");
        self.master_ids = masters.iter().copied().filter(|m| s.is(*m, b"Sprd")).enumerate().map(|(i, id)| (id, i)).collect();
        let mut spreads = Vec::new();
        for spread in s.objs(doc, b"Chld") {
            if s.is(spread, b"Sprd") {
                spreads.push(self.spread(doc, spread)?);
            } else {
                self.warn("an unknown kind of page");
            }
        }
        if spreads.is_empty() {
            return Err(Error::Malformed("document has no pages"));
        }
        let mut master_spreads = Vec::new();
        for master in masters.into_iter().filter(|m| s.is(*m, b"Sprd")) {
            master_spreads.push(self.spread(doc, master)?);
        }
        let cmyk_profile = if let Some(profile) = s.obj(root, b"CMYP") {
            let data = if let Some(name) = s.entry(profile, b"Data") {
                let size = self.archive.entry(name).ok_or(Error::Malformed("missing CMYK profile"))?.size();
                if size > MAX_PROFILE_BYTES as u64 {
                    return Err(Error::Limit("CMYK profile bytes"));
                }
                Some(self.archive.read(name)?)
            } else if let Some(data) = s.bytes(profile, b"Data") {
                if data.len() > MAX_PROFILE_BYTES {
                    return Err(Error::Limit("CMYK profile bytes"));
                }
                if !self.charge_owned(data.len()) {
                    return Err(self.failure.unwrap_or(Error::Limit("CMYK profile bytes")));
                }
                Some(data.to_vec())
            } else {
                self.warn("CMYK profile has no data; the built-in coated CMYK profile is used");
                None
            };
            data.map(std::sync::Arc::new)
        } else {
            None
        };
        if let Some(e) = self.failure {
            return Err(e);
        }
        if let Some(profile) = &cmyk_profile {
            for spread in spreads.iter_mut().chain(&mut master_spreads) {
                assign_profile(&mut spread.nodes, profile);
            }
        }
        if !self.charge_owned(0) {
            return Err(self.failure.unwrap_or(Error::Limit("model allocation")));
        }
        let warnings = std::mem::take(&mut self.warnings).into_iter().map(|(w, n)| if n > 1 { format!("{w} ({n}×)") } else { w }).collect();
        Ok(Document { dpi: self.dpi, spreads, masters: master_spreads, saved_by, warnings, cmyk_profile })
    }

    fn spread(&mut self, doc: ObjId, spread: ObjId) -> Result<Spread, Error> {
        let s = self.s;
        s.check_finite(doc, b"DfSz")?;
        s.check_finite(spread, b"SprB")?;
        if let Some([w, h]) = s.floats::<2>(doc, b"DfSz")
            && (w <= 0. || h <= 0.)
        {
            return Err(Error::Malformed("default page size"));
        }
        if let Some(v) = s.floats::<4>(spread, b"SprB")
            && (v[2] <= v[0] || v[3] <= v[1])
        {
            return Err(Error::Malformed("spread bounds"));
        }
        if let Some(md) = s.obj(spread, b"SpMd") {
            for page in s.objs(md, b"PagR") {
                s.check_finite(page, b"rctp")?;
                if let Some(v) = s.floats::<4>(page, b"rctp")
                    && (v[2] <= v[0] || v[3] <= v[1])
                {
                    return Err(Error::Malformed("page bounds"));
                }
            }
        }
        let pages = pages(s, spread);
        let default_size = s.floats::<2>(doc, b"DfSz").filter(|[w, h]| *w > 0.0 && *h > 0.0).map(|[w, h]| Rect { x0: 0.0, y0: 0.0, x1: w, y1: h });
        let bounds = s.floats::<4>(spread, b"SprB").map(rect).or_else(|| pages.iter().copied().reduce(union)).or(default_size).unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
        let transparent = s.bool(spread, b"SprT").unwrap_or(false);
        let (nodes, mask) = self.children(spread, self.base, self.budget.depth)?;
        if mask.is_some() {
            self.warn("mask layers directly on a page (imported without them)");
        }
        Ok(Spread { bounds, pages, transparent, nodes })
    }

    /// The children of `parent`, and the pixel mask a mask layer among them puts on the parent.
    fn children(&mut self, parent: ObjId, world: Affine, depth: usize) -> Result<(Vec<Node>, Option<(ObjId, Affine)>), Error> {
        let s = self.s;
        let mut out = Vec::new();
        let mut mask = None;
        if !self.charge_slots(s.array_len(parent, b"Chld")) {
            return Err(self.failure.unwrap_or(Error::Limit("array traversal slots")));
        }
        for c in s.objs(parent, b"Chld") {
            crate::import::cancelled(self.archive.cancel)?;
            if s.is(c, b"MRst") {
                // A mask layer inside a group or layer masks that container.
                if s.bool(c, b"Visi") == Some(false) {
                    continue;
                }
                let local = s.floats::<6>(c, b"Xfrm").map(Affine::from_xfrm).unwrap_or(Affine::IDENTITY);
                if mask.is_none() {
                    mask = Some((c, local.then(world)));
                } else {
                    self.warn("several pixel masks on one layer (only the first is used)");
                }
                continue;
            }
            if let Some(n) = self.node(c, world, depth + 1)? {
                out.push(n);
            }
        }
        Ok((out, mask))
    }

    fn node(&mut self, id: ObjId, parent: Affine, depth: usize) -> Result<Option<Node>, Error> {
        if let Some(e) = self.failure {
            return Err(e);
        }
        let s = self.s;
        if depth > MAX_TREE_DEPTH.min(self.archive.limits.max_depth) {
            return Err(Error::Limit("layer depth"));
        }
        self.budget.nodes += 1;
        if self.budget.nodes > MAX_NODES.min(self.archive.limits.max_objects) {
            return Err(Error::Limit("too many layers"));
        }
        // A node that contains itself (a shared object linked back into its own subtree).
        if !self.active.insert(id) {
            return Err(Error::Malformed("cyclic layer graph"));
        }
        s.check_finite(id, b"Xfrm")?;
        s.check_finite(id, b"Opac")?;
        s.check_finite(id, b"ShpB")?;
        let local = s.floats::<6>(id, b"Xfrm").map(Affine::from_xfrm).unwrap_or(Affine::IDENTITY);
        let world = local.then(parent);
        let class = s.class(id).unwrap_or(Tag(0));
        let source_name = s.str(id, b"Desc").unwrap_or_default();
        if source_name.len() > 4096 {
            return Err(Error::Limit("layer name bytes"));
        }
        if !self.charge_owned(source_name.len().saturating_add(std::mem::size_of::<Node>().saturating_mul(2))) {
            return Err(self.failure.unwrap_or(Error::Limit("layer allocation")));
        }
        self.budget.name_bytes = self.budget.name_bytes.saturating_add(source_name.len());
        if self.budget.name_bytes > MAX_NAME_BYTES {
            return Err(Error::Limit("layer names too large"));
        }
        let name = source_name.to_string();
        let visible = s.bool(id, b"Visi").unwrap_or(true);
        let locked = s.bool(id, b"Edtb").is_some_and(|e| !e);
        let opacity = s.f64(id, b"Opac").unwrap_or(1.0);
        if !opacity.is_finite() {
            return Err(Error::Malformed("layer opacity"));
        }
        let opacity = opacity.clamp(0.0, 1.0);
        let blend = match s.enumeration(id, b"Blnd") {
            None if s.bool(id, b"PasT") != Some(false) && is_container(class) => Blend::PassThrough,
            None => Blend::Normal,
            Some((i, v)) => Blend::from_enum(i, v).unwrap_or_else(|| {
                self.warn("an unknown blend mode (imported as Normal)");
                Blend::Normal
            }),
        };
        if !world.is_finite() || world.0.iter().any(|v| v.abs() > 1_000_000.) {
            return Err(Error::Malformed("invalid layer transform"));
        }
        let inherited_depth = std::mem::replace(&mut self.budget.depth, depth);
        let kind = self.kind(id, class, world, parent);
        self.budget.depth = inherited_depth;
        let kind = kind?;
        let embedded = self.pending.take();
        // A compound's children are its operands, already merged into its outline.
        let (mut children, child_mask) = if class == Tag::of(b"Comp") { (Vec::new(), None) } else { self.children(id, world, depth)? };
        if let Some(embedded) = embedded {
            children.extend(embedded);
        }
        let has_pixel_mask = child_mask.is_some() || s.array_len(id, b"AdCh") > 0;
        let over = if has_pixel_mask { mask_extent(&kind, &children) } else { None };
        let child_mask = match child_mask {
            Some((id, at)) => crate::raster::mask(self, id, at, over)?,
            None => None,
        };
        let (mask, attached_mask) = self.attached_masks(id, world, over)?;
        if attached_mask.is_some() && child_mask.is_some() {
            self.warn("attached and child pixel masks on one layer (only the attached mask is used)");
        }
        let pixel_mask = attached_mask.or(child_mask);
        self.active.remove(&id);
        if !self.charge_slots(s.array_len(id, b"FiEf")) {
            return Err(self.failure.unwrap_or(Error::Limit("array traversal slots")));
        }
        let mut node = Node { class, name, visible, locked, opacity, blend, kind, mask, pixel_mask, effects: Vec::new(), children };
        node.effects = self.effects(id, world, &node);
        Ok(Some(node))
    }

    fn kind(&mut self, id: ObjId, class: Tag, world: Affine, parent: Affine) -> Result<Kind, Error> {
        let s = self.s;
        Ok(match &class.0.to_be_bytes() {
            b"Scop" => Kind::Layer,
            b"Grup" => Kind::Group,
            b"PCrv" | b"ShpN" | b"Comp" | b"SNEN" | b"SNRR" | b"ShRN" => {
                // Affinity 3.2.3's two-board public fixture marks board shapes with an `aprp`
                // property object in `phrp`, while older documents use `ABEn`. Ordinary shapes
                // in that fixture have neither. See the pinned corpus and validation audit.
                if s.bool(id, b"ABEn") == Some(true) || s.obj(id, b"phrp").is_some_and(|p| s.is(p, b"aprp")) {
                    return Ok(self.artboard(id, world));
                }
                let Some((local, even_odd)) = crate::geometry::outline(self, id, class, world) else {
                    self.warn(format!("a {} that could not be read", describe(class)));
                    return Ok(Kind::Unsupported);
                };
                let (fills, strokes) = paint::node_paint(self, id, world);
                Kind::Shape { path: local.transformed(world), fills, strokes, even_odd }
            }
            b"TxtT" => match crate::text::table(self, id, world) {
                Some(table) => Kind::Table(table),
                None => {
                    self.warn("tables that could not be read");
                    Kind::Unsupported
                }
            },
            b"MPIN" => {
                let link = s.obj(id, b"SLnk");
                if !self.charge_slots(link.map_or(0, |l| s.array_len(l, b"ILOb"))) {
                    return Err(self.failure.unwrap_or(Error::Limit("master links")));
                }
                let master = link.into_iter().flat_map(|l| s.objs(l, b"ILOb")).find_map(|o| self.master_ids.get(&o).copied());
                let at = |tag, default| s.int(id, tag).and_then(|v| usize::try_from(v).ok()).unwrap_or(default).min(4096);
                match master {
                    Some(master) if self.embed_depth == 0 => {
                        Kind::MasterInstance { master, page: at(b"PgOf", 0), master_page: at(b"MPOf", 0), count: at(b"PgCt", 1) }
                    }
                    _ => {
                        self.warn("master pages of embedded documents or missing masters (left out)");
                        Kind::Unsupported
                    }
                }
            }
            b"TxtA" | b"TxtF" | b"TxtC" => match crate::text::read(self, id, world) {
                Some(t) => Kind::Text(t),
                None => {
                    self.warn("text whose story could not be read (master-page text, or an unknown layout)");
                    Kind::Unsupported
                }
            },
            b"ImgN" | b"Rstr" => match crate::raster::node_image(self, id, world)? {
                Some(img) => Kind::Image(img),
                None => Kind::Unsupported,
            },
            b"EmbN" => {
                let embedded = match self.embedded_document(id, world) {
                    Ok(nodes) => nodes,
                    // Cancellation, exhausted shared budgets and failed integrity checks cannot
                    // be recovered by consuming more input for a cached picture.
                    Err(error @ (Error::Cancelled | Error::Limit(_) | Error::Malformed("archive entry checksum mismatch"))) => return Err(error),
                    Err(error) => {
                        self.warn(format!("embedded documents that could not be read ({error}); cached picture used when available"));
                        None
                    }
                };
                if let Some(nodes) = embedded {
                    self.pending = Some(nodes);
                    Kind::Group
                } else {
                    match crate::raster::embedded(self, id, world)? {
                        Some(img) => {
                            self.warn("embedded documents and symbols (imported as pictures of them)");
                            Kind::Image(img)
                        }
                        None => {
                            self.warn("embedded documents and symbols without a cached picture");
                            Kind::Unsupported
                        }
                    }
                }
            }
            b"FRst" => {
                // A fill layer paints its whole bitmap area; its gradient lives in page space.
                let b = s.obj(id, b"Bitm");
                let size = |t: &[u8; 4]| b.and_then(|b| s.int(b, t)).map(|v| v as f64).filter(|v| *v > 0.0);
                match (size(b"BmpW"), size(b"BmpH")) {
                    (Some(w), Some(h)) => {
                        let local = Path { subpaths: vec![crate::shapes::rectangle(0.0, 0.0, w, h)] };
                        let fills = paint::fills(self, id, parent, b"BFFl");
                        Kind::Shape { path: local.transformed(world), fills, strokes: Vec::new(), even_odd: false }
                    }
                    _ => {
                        self.warn("fill layers without an area");
                        Kind::Unsupported
                    }
                }
            }
            _ => {
                self.warn(format!("{} layers", describe(class)));
                if s.obj(id, b"Bitm").is_some() {
                    if let Some(image) = crate::raster::node_image(self, id, world)? {
                        self.warn("unsupported records with cached pixels (imported as raster pictures)");
                        Kind::Image(image)
                    } else {
                        Kind::Unsupported
                    }
                } else {
                    Kind::Unsupported
                }
            }
        })
    }

    /// The enabled layer effects (`FiEf`) of node `id`. The angle convention was measured on the
    /// public MIT fx fixture (Patchy): an angle of π/2 with offset 10 shadows 10 below.
    fn effects(&mut self, id: ObjId, world: Affine, node: &Node) -> Vec<Effect> {
        let s = self.s;
        let mut out = Vec::new();
        if self.failure.is_some() {
            return out;
        }
        if s.array_len(id, b"FiEf") > 64 {
            self.failure = Some(Error::Limit("layer effects"));
            return out;
        }
        if !self.charge_owned(s.array_len(id, b"FiEf").saturating_mul(std::mem::size_of::<Effect>())) {
            return out;
        }
        let mut bounds = None;
        for e in s.objs(id, b"FiEf").into_iter().take(64) {
            if s.bool(e, b"Enab") == Some(false) {
                continue;
            }
            for tag in [b"Radi", b"Offs", b"Opac", b"Angl", b"Azim", b"Dept", b"ShOp"] {
                if let Err(error) = s.check_finite(e, tag) {
                    self.failure = Some(error);
                    return out;
                }
            }
            if [b"Radi", b"Offs"].iter().any(|tag| s.f64(e, tag).is_some_and(|v| !(0.0..1e6).contains(&v))) {
                self.failure = Some(Error::Limit("effect geometry"));
                return out;
            }
            if [b"Angl", b"Azim"].iter().any(|tag| s.f64(e, tag).is_some_and(|v| v.abs() > 1e6)) {
                self.failure = Some(Error::Limit("effect angle"));
                return out;
            }
            // "Scale with object": lengths follow the node's transform; otherwise they are absolute.
            let k = if s.bool(e, b"SclO") == Some(true) { world.scale() } else { 1.0 };
            let len = |t: &[u8; 4]| s.f64(e, t).filter(|v| v.is_finite() && *v >= 0.0 && *v < 1e6).unwrap_or(0.0) * k;
            let opacity = s.f64(e, b"Opac").filter(|v| v.is_finite()).unwrap_or(1.0).clamp(0.0, 1.0);
            let angle = s.f64(e, b"Angl").filter(|v| v.is_finite()).unwrap_or(0.0);
            let color = s.obj(e, b"Colr").and_then(|c| crate::paint::color(self, c));
            let class = s.class(e).map(|c| c.0.to_be_bytes());
            match (class.as_ref(), color) {
                (Some(b"Shad"), Some(color)) => out.push(Effect::DropShadow { color, opacity, radius: len(b"Radi"), distance: len(b"Offs"), angle }),
                (Some(b"InnS"), Some(color)) => out.push(Effect::InnerShadow { color, opacity, radius: len(b"Radi"), distance: len(b"Offs"), angle }),
                (Some(b"OutG"), Some(color)) => out.push(Effect::OuterGlow { color, opacity, radius: len(b"Radi") }),
                (Some(b"Strk"), Some(color)) if s.enumeration(e, b"Ftyp").is_none_or(|(t, _)| t == 0) => {
                    let align = s.enumeration(e, b"Alig").map_or(0, |(a, _)| a);
                    out.push(Effect::Outline { color, opacity, width: len(b"Radi"), align });
                }
                (Some(b"Strk"), _) => self.warn("gradient outline effects (left out)"),
                (Some(b"BevE" | b"EmbE"), _) => {
                    // No radius, no bevel (as Affinity draws it).
                    let size = len(b"Radi");
                    if size > 0.0 {
                        let white = crate::paint::Color::Rgb { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
                        let black = crate::paint::Color::Rgb { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
                        let highlight = s.obj(e, b"HiCl").and_then(|c| crate::paint::color(self, c)).unwrap_or(white);
                        let shadow = s.obj(e, b"ShCl").and_then(|c| crate::paint::color(self, c)).unwrap_or(black);
                        let shadow_opacity = s.f64(e, b"ShOp").filter(|v| v.is_finite()).unwrap_or(0.75).clamp(0.0, 1.0);
                        let depth = s.f64(e, b"Dept").filter(|v| v.is_finite()).unwrap_or(5.0);
                        let azimuth = s.f64(e, b"Azim").filter(|v| v.is_finite()).unwrap_or(std::f64::consts::FRAC_PI_4 * 3.0);
                        let inner = s.enumeration(e, b"Beve").is_none_or(|(k, _)| k == 0);
                        out.push(Effect::Bevel { size, depth, angle: azimuth, highlight, highlight_opacity: opacity, shadow, shadow_opacity, inner });
                    }
                }
                (Some(b"ColO"), Some(color)) => {
                    let blend = self.effect_blend(e);
                    out.push(Effect::ColorOverlay { color, opacity, blend });
                }
                (Some(b"GrdO"), _) => {
                    let blend = self.effect_blend(e);
                    // The gradient's unit space spans the node's bounding box from -1 to 1, centred
                    // on it (y down): checked against the stored previews of real documents.
                    let space = bounds
                        .get_or_insert_with(|| tree_bounds(node))
                        .map(|b| Affine([(b.x1 - b.x0) / 2.0, 0.0, 0.0, (b.y1 - b.y0) / 2.0, (b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0]));
                    match (s.obj(e, b"GrFl"), space) {
                        (Some(fill), Some(space)) => match paint::descriptor(self, fill, space) {
                            Some(Paint::Gradient(gradient)) => {
                                if !self.charge_owned(gradient.stops.len().saturating_mul(std::mem::size_of::<paint::Stop>())) {
                                    return out;
                                }
                                out.push(Effect::GradientOverlay { gradient, opacity, blend });
                            }
                            Some(Paint::Solid(color)) => out.push(Effect::ColorOverlay { color, opacity, blend }),
                            _ => {}
                        },
                        // Nothing visible to lay it over.
                        (Some(_), None) => {}
                        (None, _) => self.warn("gradient overlay effects that could not be read (left out)"),
                    }
                }
                (Some(b"ColO"), None) => self.warn("colour overlay effects that could not be read (left out)"),
                (Some(b"Gaus"), _) => self.warn("blur effects (left out)"),
                _ => self.warn("layer effects of other kinds (left out)"),
            }
        }
        out
    }

    /// A layer effect's blend mode (`BlnM`); unknown modes are Normal (warned).
    fn effect_blend(&mut self, e: ObjId) -> Blend {
        match self.s.enumeration(e, b"BlnM") {
            None => Blend::Normal,
            Some((i, v)) => Blend::from_enum(i, v).unwrap_or_else(|| {
                self.warn("an unknown blend mode (imported as Normal)");
                Blend::Normal
            }),
        }
    }

    /// Vector shapes attached to a node (`AdCh`) mask it: it shows only inside their outlines;
    /// an attached mask layer is a pixel mask. Adjustments attached the same way are reported.
    fn attached_masks(&mut self, id: ObjId, world: Affine, over: Option<Rect>) -> Result<(Option<Path>, Option<Image>), Error> {
        let s = self.s;
        if !self.charge_slots(s.array_len(id, b"AdCh")) {
            return Err(self.failure.unwrap_or(Error::Limit("array traversal slots")));
        }
        let attached = s.objs(id, b"AdCh");
        let mut mask: Option<Path> = None;
        let mut pixels: Option<Image> = None;
        for a in attached {
            let class = s.class(a).unwrap_or(Tag(0));
            if s.bool(a, b"Visi") == Some(false) {
                continue;
            }
            if class == Tag::of(b"PCrv") || class == Tag::of(b"ShpN") || class == Tag::of(b"Comp") {
                let local = s.floats::<6>(a, b"Xfrm").map(Affine::from_xfrm).unwrap_or(Affine::IDENTITY).then(world);
                if let Some((p, _)) = crate::geometry::outline(self, a, class, local) {
                    mask.get_or_insert_with(Path::default).subpaths.extend(p.transformed(local).subpaths);
                }
            } else if class == Tag::of(b"MRst") {
                let local = s.floats::<6>(a, b"Xfrm").map(Affine::from_xfrm).unwrap_or(Affine::IDENTITY).then(world);
                match crate::raster::mask(self, a, local, over)? {
                    Some(m) if pixels.is_none() => pixels = Some(m),
                    Some(_) => self.warn("several pixel masks on one layer (only the first is used)"),
                    None => {}
                }
            } else {
                self.warn(format!("{} layers attached to other layers (imported without them)", describe(class)));
            }
        }
        Ok((mask, pixels))
    }

    fn embedded_document(&mut self, id: ObjId, world: Affine) -> Result<Option<Vec<Node>>, Error> {
        let s = self.s;
        let Some(entry) = s.obj(id, b"Bitm").and_then(|b| s.obj(b, b"EmCn")).and_then(|c| s.entry(c, b"EmbC")) else { return Ok(None) };
        if entry.is_empty() {
            return Ok(None);
        }
        if self.embed_depth >= MAX_EMBED_DEPTH {
            return Err(Error::Limit("embedded document depth"));
        }
        let data = self.archive.read(entry)?;
        let Some(inner) = data.strip_prefix(b"EmDc").and_then(|d| d.get(4..)).filter(|d| crate::is_affinity(d)) else { return Ok(None) };
        let probe = self.read_embedded(inner, Affine::IDENTITY)?;
        let Some(probe) = probe else { return Ok(None) };
        let spread = probe.spreads.first().ok_or(Error::Malformed("embedded pages"))?;
        let content = s.obj(id, b"Bitm").and_then(|b| s.enumeration(b, b"PBBx")).is_some_and(|(k, _)| k == 2);
        let board = spread.nodes.iter().find_map(|n| if let Kind::Artboard { rect, .. } = &n.kind { Some(*rect) } else { None });
        let page = (if content { None } else { spread.pages.first().copied().or(board) }).or_else(|| nodes_bounds(&spread.nodes)).unwrap_or(spread.bounds);
        drop(probe);
        let base = Affine([1., 0., 0., 1., -(page.x0 + page.x1) / 2., -(page.y0 + page.y1) / 2.]).then(world);
        if !base.is_finite() {
            return Err(Error::Malformed("embedded placement"));
        }
        let Some(doc) = self.read_embedded(inner, base)? else { return Ok(None) };
        for warning in doc.warnings {
            self.warn(format!("{warning} [in embedded documents]"));
        }
        if doc.spreads.len() > 1 {
            self.warn("embedded documents with several pages (the first is used)");
        }
        Ok(doc.spreads.into_iter().next().map(|s| s.nodes))
    }

    fn read_embedded(&mut self, bytes: &[u8], base: Affine) -> Result<Option<Document>, Error> {
        let mut limits = self.archive.limits;
        limits.max_total = limits.max_total.saturating_sub(self.archive.extracted());
        let mut archive = Archive::open_with(bytes, limits, self.archive.cancel)?;
        let result = (|| {
            let data = archive.read("doc.dat")?;
            let stream_limits = Limits { max_depth: self.stream_depth, ..limits };
            let stream = stream::parse_with(&data, stream_limits, archive.cancel)?;
            let mut reader = Reader::new(&stream, &mut archive);
            reader.budget = self.budget;
            reader.stream_depth = self.stream_depth;
            reader.segments = self.segments;
            reader.base = base;
            reader.type_base = Affine([base.0[0], base.0[1], base.0[2], base.0[3], 0., 0.]);
            reader.embed_depth = self.embed_depth + 1;
            reader.archive_base = self.archive_base.saturating_add(self.archive.extracted());
            let result = reader.document();
            self.budget = reader.budget;
            self.segments = reader.segments;
            result
        })();
        self.archive.charge(archive.extracted())?;
        if !self.charge_owned(0) {
            return Err(self.failure.unwrap_or(Error::Limit("nested model allocation")));
        }
        match result {
            Ok(doc) => Ok(Some(doc)),
            Err(Error::Unsupported(reason)) => {
                self.warn(format!("embedded document unsupported ({reason})"));
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn artboard(&mut self, id: ObjId, world: Affine) -> Kind {
        let s = self.s;
        let Some(b) = s.floats::<4>(id, b"ShpB").map(rect).filter(|b| b.x1 > b.x0 && b.y1 > b.y0) else {
            self.warn("an artboard without valid bounds (its children are kept without clipping)");
            return Kind::Unsupported;
        };
        let corners = [Point { x: b.x0, y: b.y0 }, Point { x: b.x1, y: b.y0 }, Point { x: b.x1, y: b.y1 }, Point { x: b.x0, y: b.y1 }].map(|p| world.apply(p));
        let [a, b2, c, d, ..] = world.0;
        if b2.abs() > 1e-9 || c.abs() > 1e-9 || a <= 0.0 || d <= 0.0 {
            self.warn("a rotated or flipped artboard (export bounds are its bounding box; artwork keeps its outline)");
        }
        let xs = corners.map(|p| p.x);
        let ys = corners.map(|p| p.y);
        let rect = Rect {
            x0: xs.iter().copied().fold(f64::INFINITY, f64::min),
            y0: ys.iter().copied().fold(f64::INFINITY, f64::min),
            x1: xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            y1: ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        };
        if ![rect.x0, rect.y0, rect.x1, rect.y1].iter().all(|v| v.is_finite()) || rect.x1 <= rect.x0 || rect.y1 <= rect.y0 {
            self.warn("an artboard with an invalid transformed size (its children are kept without clipping)");
            return Kind::Unsupported;
        }
        let class = s.class(id).unwrap_or(Tag::of(b"ShpN"));
        let (path, even_odd) = crate::geometry::outline(self, id, class, world).unwrap_or_else(|| {
            self.warn("an artboard whose outline could not be read (clipped to its rectangle)");
            (Path { subpaths: vec![crate::shapes::rectangle(b.x0, b.y0, b.x1, b.y1)] }, false)
        });
        let (background, strokes) = paint::node_paint(self, id, world);
        Kind::Artboard { rect, path: path.transformed(world), even_odd, background, strokes }
    }
}

/// The bounds of what visible `nodes` draw (control points of outlines, corners of images, frames
/// and tables), in document pixels.
fn nodes_bounds(nodes: &[Node]) -> Option<Rect> {
    let mut out = None;
    walk_bounds(nodes, true, 0, &mut out);
    out
}

/// The bounds of `node` and everything in it, hidden or not.
fn tree_bounds(node: &Node) -> Option<Rect> {
    let mut out = None;
    walk_bounds(std::slice::from_ref(node), false, 0, &mut out);
    out
}

fn walk_bounds(nodes: &[Node], visible_only: bool, depth: usize, out: &mut Option<Rect>) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    for n in nodes.iter().filter(|n| n.visible || !visible_only) {
        kind_points(&n.kind, &mut |p| add_point(out, p));
        walk_bounds(&n.children, visible_only, depth + 1, out);
    }
}

/// Pixel masks cover known artwork bounds, including stroke reach. Artistic glyph bounds
/// require layout, so a subtree containing artistic text keeps the whole mask.
fn mask_extent(kind: &Kind, children: &[Node]) -> Option<Rect> {
    fn visit(kind: &Kind, children: &[Node], depth: usize, out: &mut Option<Rect>) -> bool {
        if depth > MAX_TREE_DEPTH || matches!(kind, Kind::Text(t) if t.frame.is_none()) {
            return false;
        }
        let mut own = None;
        kind_points(kind, &mut |p| add_point(&mut own, p));
        if let (Some(r), Kind::Shape { strokes, .. } | Kind::Artboard { strokes, .. }) = (own, kind) {
            let reach = strokes.iter().map(|s| s.width * s.miter_limit.clamp(1.0, 10.0)).filter(|w| w.is_finite()).fold(0.0, f64::max);
            own = Some(Rect { x0: r.x0 - reach, y0: r.y0 - reach, x1: r.x1 + reach, y1: r.y1 + reach });
        }
        if let Some(r) = own {
            *out = Some(out.map_or(r, |o| union(o, r)));
        }
        children.iter().filter(|n| n.visible).all(|n| visit(&n.kind, &n.children, depth + 1, out))
    }
    let mut out = None;
    if visit(kind, children, 0, &mut out) { out } else { None }
}

fn add_point(out: &mut Option<Rect>, p: Point) {
    if p.x.is_finite() && p.y.is_finite() {
        let r = Rect { x0: p.x, y0: p.y, x1: p.x, y1: p.y };
        *out = Some(out.map_or(r, |o| union(o, r)));
    }
}

fn kind_points(kind: &Kind, add: &mut dyn FnMut(Point)) {
    let corners = |r: Rect, m: Affine| [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)].map(|(x, y)| m.apply(Point { x, y }));
    match kind {
        Kind::Shape { path, .. } | Kind::Artboard { path, .. } => {
            for sp in &path.subpaths {
                add(sp.start);
                for seg in &sp.segments {
                    seg.iter().for_each(|p| add(*p));
                }
            }
        }
        Kind::Image(img) => corners(Rect { x0: 0.0, y0: 0.0, x1: f64::from(img.width), y1: f64::from(img.height) }, img.transform).into_iter().for_each(add),
        Kind::Text(t) => match t.frame {
            Some(f) => corners(f, t.transform).into_iter().for_each(add),
            None => add(t.transform.apply(t.anchor)),
        },
        Kind::Table(t) => {
            if let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) = (t.columns.first(), t.columns.last(), t.rows.first(), t.rows.last()) {
                corners(Rect { x0, y0, x1, y1 }, t.transform).into_iter().for_each(add);
            }
        }
        Kind::Layer | Kind::Group | Kind::Unsupported | Kind::MasterInstance { .. } => {}
    }
}

/// Page rectangles: `SpMd.PagR[].rctp` (Publisher 2, Affinity 3) or `SprB` split into `PagC`
/// equal pages (Publisher 1 facing spreads).
fn pages(s: &Stream, spread: ObjId) -> Vec<Rect> {
    if let Some(md) = s.obj(spread, b"SpMd") {
        let v: Vec<Rect> = s.objs(md, b"PagR").into_iter().filter_map(|p| s.floats::<4>(p, b"rctp").map(rect)).filter(|r| r.x1 > r.x0 && r.y1 > r.y0).collect();
        if !v.is_empty() {
            return v;
        }
    }
    match (s.floats::<4>(spread, b"SprB").map(rect), s.int(spread, b"PagC")) {
        (Some(b), Some(n @ 2..=16)) => {
            let w = (b.x1 - b.x0) / n as f64;
            (0..n).map(|i| Rect { x0: b.x0 + w * i as f64, x1: b.x0 + w * (i + 1) as f64, ..b }).collect()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn union(a: Rect, b: Rect) -> Rect {
    Rect { x0: a.x0.min(b.x0), y0: a.y0.min(b.y0), x1: a.x1.max(b.x1), y1: a.y1.max(b.y1) }
}

fn is_container(class: Tag) -> bool {
    class == Tag::of(b"Scop") || class == Tag::of(b"Grup")
}

pub(crate) fn rect(v: [f64; 4]) -> Rect {
    Rect { x0: v[0], y0: v[1], x1: v[2], y1: v[3] }
}

/// A readable name for a layer class in warnings.
pub(crate) fn describe(class: Tag) -> &'static str {
    match &class.0.to_be_bytes() {
        b"PCrv" => "curve",
        b"ShpN" | b"SNEN" | b"SNRR" | b"ShRN" => "shape",
        b"Comp" => "compound",
        b"Rstr" => "pixel",
        b"ImgN" => "image",
        b"EmbN" => "embedded document",
        b"MPIN" => "master page instance",
        b"FRst" => "fill",
        b"MRst" => "mask",
        b"Slic" | b"SlcP" => "slice",
        b"3DLA" => "3D",
        [_, _, b'R', b'A'] => "adjustment",
        _ => "unknown",
    }
}

fn assign_profile(nodes: &mut [Node], profile: &std::sync::Arc<Vec<u8>>) {
    for node in nodes {
        if let Kind::Image(image) = &mut node.kind
            && matches!(image.pixels, Pixels::Cmyka8(_))
            && image.profile.is_none()
        {
            image.profile = Some(profile.clone());
        }
        if node.class != Tag::of(b"EmbN") {
            assign_profile(&mut node.children, profile);
        }
    }
}
