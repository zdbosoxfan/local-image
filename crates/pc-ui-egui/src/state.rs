//! UI state as plain data, so the control channel (and later MCP) can read and drive every
//! aspect of the interface: tools, zoom, panels, dialogs and windows.

use serde::{Deserialize, Serialize};

/// Shared selection and capture for a point curve; document parameters remain with the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PointCurveState {
    pub selected: Option<usize>,
    /// Captured point (None while removed outside), grab offset, and re-entry eligibility.
    pub drag: Option<PointCurveDrag>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PointCurveDrag {
    pub index: Option<usize>,
    pub offset: [f32; 2],
    pub removed: bool,
}

/// Curves editor memory (egui temp data): the edited channel and the point gesture.
#[derive(Clone, Copy, Debug, Default)]
pub struct CurvesEditorState {
    pub channel: usize,
    pub gesture: PointCurveState,
}

/// Camera Raw scope preferences are view state, never filter parameters or document history.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraRawScopeState {
    pub shadows: bool,
    pub highlights: bool,
    pub lab: bool,
    pub sampler_tool: bool,
    pub samplers: Vec<[f32; 2]>,
    pub vectorscope: bool,
    pub selected_region: bool,
    pub red_right: bool,
    pub hide_skin_line: bool,
    pub floating: bool,
    pub floating_rect: Option<[f32; 4]>,
}

/// Camera Raw navigation, separate from filter settings. Reset for each opened image.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraRawPreviewState {
    /// None fits the image; Some is physical display pixels per source pixel.
    pub zoom: Option<f32>,
    pub center: [f32; 2],
    pub hand: bool,
}

impl Default for CameraRawPreviewState {
    fn default() -> Self {
        Self { zoom: None, center: [0.5, 0.5], hand: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    Move,
    RectMarquee,
    EllipseMarquee,
    Lasso,
    PolygonLasso,
    MagneticLasso,
    MagicWand,
    Crop,
    Eyedropper,
    Ruler,
    Note,
    Count,
    Brush,
    Pencil,
    MixerBrush,
    Eraser,
    BackgroundEraser,
    MagicEraser,
    Gradient,
    PaintBucket,
    Type,
    VerticalType,
    Hand,
    RotateView,
    Zoom,
    SpotHealing,
    Healing,
    Patch,
    ContentAwareMove,
    /// local-image: the Red Eye tool.
    RedEye,
    CloneStamp,
    HistoryBrush,
    Blur,
    Sharpen,
    Smudge,
    Dodge,
    Burn,
    Sponge,
    QuickSelection,
    ObjectSelection,
    Pen,
    PathSelection,
    DirectSelection,
    Rectangle,
    EllipseShape,
    Triangle,
    Polygon,
    Line,
    CustomShape,
    Slice,
    SliceSelect,
    // local-image: Perspective Crop (C group).
    PerspectiveCrop,
    // local-image: AI tools (see ai_ui.rs).
    AiRemove,
    AiCutout,
    PointTransform,
    VectorPencil,
    VectorBrush,
    Smooth,
    PathEraser,
    Join,
    Corner,
    RoundedRectangle,
    Star,
    DoubleStar,
    SquareStar,
    Trapezoid,
    Diamond,
    Donut,
    Pie,
    Segment,
    Arrow,
    Callout,
    Cog,
    Heart,
    Tear,
    Crescent,
    Cloud,
    Spiral,
    Arc,
    RectGrid,
    PolarGrid,
    VectorFill,
    Transparency,
    ShapeBuilder,
    Contour,
    Knife,
    Scissors,
    LineCut,
    Width,
    ArtText,
    FrameText,
    TextOnPath,
    Blend,
    Repeat,
    Envelope,
    VectorCrop,
    Artboard,
    GroupSelection,
    VectorMagicWand,
    VectorLasso,
    Curvature,
    AddAnchor,
    DeleteAnchor,
    AnchorPoint,
    Rotate,
    Reflect,
    Scale,
    Shear,
    Reshape,
    FreeTransform,
    StylePicker,
    BlobBrush,
    VectorEraser,
    VectorWarp,
    VectorTwirl,
    VectorPucker,
    VectorBloat,
    Scallop,
    Crystallize,
    Wrinkle,
    VectorPuppetWarp,
    LivePaintBucket,
    VectorFloodFill,
    LivePaintSelection,
    Mesh,
    Measure,
    Area,
}

impl Tool {
    pub const ALL: [Tool; 127] = [
        Tool::Move,
        Tool::RectMarquee,
        Tool::EllipseMarquee,
        Tool::Lasso,
        Tool::PolygonLasso,
        Tool::MagneticLasso,
        Tool::MagicWand,
        Tool::Crop,
        Tool::PerspectiveCrop,
        Tool::Eyedropper,
        Tool::Ruler,
        Tool::Note,
        Tool::Count,
        Tool::Brush,
        Tool::Pencil,
        Tool::MixerBrush,
        Tool::Eraser,
        Tool::BackgroundEraser,
        Tool::MagicEraser,
        Tool::Gradient,
        Tool::PaintBucket,
        Tool::Type,
        Tool::VerticalType,
        Tool::Hand,
        Tool::RotateView,
        Tool::Zoom,
        Tool::SpotHealing,
        Tool::Healing,
        Tool::Patch,
        Tool::ContentAwareMove,
        Tool::RedEye,
        Tool::CloneStamp,
        Tool::HistoryBrush,
        Tool::Blur,
        Tool::Sharpen,
        Tool::Smudge,
        Tool::Dodge,
        Tool::Burn,
        Tool::Sponge,
        Tool::QuickSelection,
        Tool::ObjectSelection,
        Tool::Pen,
        Tool::PathSelection,
        Tool::DirectSelection,
        Tool::Rectangle,
        Tool::EllipseShape,
        Tool::Triangle,
        Tool::Polygon,
        Tool::Line,
        Tool::CustomShape,
        Tool::Slice,
        Tool::SliceSelect,
        Tool::AiRemove,
        Tool::AiCutout,
        Tool::PointTransform,
        Tool::VectorPencil,
        Tool::VectorBrush,
        Tool::Smooth,
        Tool::PathEraser,
        Tool::Join,
        Tool::Corner,
        Tool::RoundedRectangle,
        Tool::Star,
        Tool::DoubleStar,
        Tool::SquareStar,
        Tool::Trapezoid,
        Tool::Diamond,
        Tool::Donut,
        Tool::Pie,
        Tool::Segment,
        Tool::Arrow,
        Tool::Callout,
        Tool::Cog,
        Tool::Heart,
        Tool::Tear,
        Tool::Crescent,
        Tool::Cloud,
        Tool::Spiral,
        Tool::Arc,
        Tool::RectGrid,
        Tool::PolarGrid,
        Tool::VectorFill,
        Tool::Transparency,
        Tool::ShapeBuilder,
        Tool::Contour,
        Tool::Knife,
        Tool::Scissors,
        Tool::LineCut,
        Tool::Width,
        Tool::ArtText,
        Tool::FrameText,
        Tool::TextOnPath,
        Tool::Blend,
        Tool::Repeat,
        Tool::Envelope,
        Tool::VectorCrop,
        Tool::Artboard,
        Tool::GroupSelection,
        Tool::VectorMagicWand,
        Tool::VectorLasso,
        Tool::Curvature,
        Tool::AddAnchor,
        Tool::DeleteAnchor,
        Tool::AnchorPoint,
        Tool::Rotate,
        Tool::Reflect,
        Tool::Scale,
        Tool::Shear,
        Tool::Reshape,
        Tool::FreeTransform,
        Tool::StylePicker,
        Tool::BlobBrush,
        Tool::VectorEraser,
        Tool::VectorWarp,
        Tool::VectorTwirl,
        Tool::VectorPucker,
        Tool::VectorBloat,
        Tool::Scallop,
        Tool::Crystallize,
        Tool::Wrinkle,
        Tool::VectorPuppetWarp,
        Tool::LivePaintBucket,
        Tool::VectorFloodFill,
        Tool::LivePaintSelection,
        Tool::Mesh,
        Tool::Measure,
        Tool::Area,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => "Move Tool",
            Tool::RectMarquee => "Rectangular Marquee Tool",
            Tool::EllipseMarquee => "Elliptical Marquee Tool",
            Tool::Brush => "Brush Tool",
            Tool::Pencil => "Pencil Tool",
            Tool::MixerBrush => "Mixer Brush Tool",
            Tool::Eraser => "Eraser Tool",
            Tool::BackgroundEraser => "Background Eraser Tool",
            Tool::MagicEraser => "Magic Eraser Tool",
            Tool::Eyedropper => "Eyedropper Tool",
            Tool::Ruler => "Ruler Tool",
            Tool::Note => "Note Tool",
            Tool::Count => "Count Tool",
            Tool::Lasso => "Lasso Tool",
            Tool::PolygonLasso => "Polygonal Lasso Tool",
            Tool::MagneticLasso => "Magnetic Lasso Tool",
            Tool::MagicWand => "Magic Wand Tool",
            Tool::Crop => "Crop Tool",
            Tool::Slice => "Slice Tool",
            Tool::SliceSelect => "Slice Select Tool",
            Tool::PerspectiveCrop => "Perspective Crop Tool",
            Tool::Gradient => "Gradient Tool",
            Tool::PaintBucket => "Paint Bucket Tool",
            Tool::Type => "Horizontal Type Tool",
            Tool::VerticalType => "Vertical Type Tool",
            Tool::Hand => "Hand Tool",
            Tool::RotateView => "Rotate View Tool",
            Tool::Zoom => "Zoom Tool",
            Tool::SpotHealing => "Spot Healing Brush Tool",
            Tool::Healing => "Healing Brush Tool",
            Tool::Patch => "Patch Tool",
            Tool::ContentAwareMove => "Content-Aware Move Tool",
            Tool::RedEye => "Red Eye Tool",
            Tool::CloneStamp => "Clone Stamp Tool",
            Tool::HistoryBrush => "History Brush Tool",
            Tool::Blur => "Blur Tool",
            Tool::Sharpen => "Sharpen Tool",
            Tool::Smudge => "Smudge Tool",
            Tool::Dodge => "Dodge Tool",
            Tool::Burn => "Burn Tool",
            Tool::Sponge => "Sponge Tool",
            Tool::QuickSelection => "Quick Selection Tool",
            Tool::ObjectSelection => "Object Selection Tool",
            Tool::Pen => "Pen Tool",
            Tool::PathSelection => "Path Selection Tool",
            Tool::DirectSelection => "Direct Selection Tool",
            Tool::Rectangle => "Rectangle Tool",
            Tool::EllipseShape => "Ellipse Tool",
            Tool::Triangle => "Triangle Tool",
            Tool::Polygon => "Polygon Tool",
            Tool::Line => "Line Tool",
            Tool::CustomShape => "Custom Shape Tool",
            Tool::AiRemove => "AI Remove Brush Tool",
            Tool::AiCutout => "AI Cutout Tool",
            Tool::PointTransform => "Point Transform Tool",
            Tool::VectorPencil => "Vector Pencil Tool",
            Tool::VectorBrush => "Vector Brush Tool",
            Tool::Smooth => "Smooth Tool",
            Tool::PathEraser => "Path Eraser Tool",
            Tool::Join => "Join Tool",
            Tool::Corner => "Corner Tool",
            Tool::RoundedRectangle => "Rounded Rectangle Tool",
            Tool::Star => "Star Tool",
            Tool::DoubleStar => "Double Star Tool",
            Tool::SquareStar => "Square Star Tool",
            Tool::Trapezoid => "Trapezoid Tool",
            Tool::Diamond => "Diamond Tool",
            Tool::Donut => "Donut Tool",
            Tool::Pie => "Pie Tool",
            Tool::Segment => "Segment Tool",
            Tool::Arrow => "Arrow Tool",
            Tool::Callout => "Callout Tool",
            Tool::Cog => "Cog Tool",
            Tool::Heart => "Heart Tool",
            Tool::Tear => "Tear Tool",
            Tool::Crescent => "Crescent Tool",
            Tool::Cloud => "Cloud Tool",
            Tool::Spiral => "Spiral Tool",
            Tool::Arc => "Arc Tool",
            Tool::RectGrid => "Rectangular Grid Tool",
            Tool::PolarGrid => "Polar Grid Tool",
            Tool::VectorFill => "Vector Fill Tool",
            Tool::Transparency => "Transparency Tool",
            Tool::ShapeBuilder => "Shape Builder Tool",
            Tool::Contour => "Contour Tool",
            Tool::Knife => "Knife Tool",
            Tool::Scissors => "Scissors Tool",
            Tool::LineCut => "Line Cut Tool",
            Tool::Width => "Width Tool",
            Tool::ArtText => "Artistic Text Tool",
            Tool::FrameText => "Frame Text Tool",
            Tool::TextOnPath => "Text on Path Tool",
            Tool::Blend => "Blend Tool",
            Tool::Repeat => "Repeat Tool",
            Tool::Envelope => "Envelope Tool",
            Tool::VectorCrop => "Vector Crop Tool",
            Tool::Artboard => "Artboard Tool",
            Tool::GroupSelection => "Group Selection Tool",
            Tool::VectorMagicWand => "Vector Magic Wand Tool",
            Tool::VectorLasso => "Vector Lasso Tool",
            Tool::Curvature => "Curvature Tool",
            Tool::AddAnchor => "Add Anchor Tool",
            Tool::DeleteAnchor => "Delete Anchor Tool",
            Tool::AnchorPoint => "Anchor Point Tool",
            Tool::Rotate => "Rotate Tool",
            Tool::Reflect => "Reflect Tool",
            Tool::Scale => "Scale Tool",
            Tool::Shear => "Shear Tool",
            Tool::Reshape => "Reshape Tool",
            Tool::FreeTransform => "Free Transform Tool",
            Tool::StylePicker => "Style Picker Tool",
            Tool::BlobBrush => "Blob Brush Tool",
            Tool::VectorEraser => "Vector Eraser Tool",
            Tool::VectorWarp => "Warp Tool",
            Tool::VectorTwirl => "Twirl Tool",
            Tool::VectorPucker => "Pucker Tool",
            Tool::VectorBloat => "Bloat Tool",
            Tool::Scallop => "Scallop Tool",
            Tool::Crystallize => "Crystallize Tool",
            Tool::Wrinkle => "Wrinkle Tool",
            Tool::VectorPuppetWarp => "Puppet Warp Tool",
            Tool::LivePaintBucket => "Live Paint Bucket Tool",
            Tool::VectorFloodFill => "Vector Flood Fill Tool",
            Tool::LivePaintSelection => "Live Paint Selection Tool",
            Tool::Mesh => "Mesh Tool",
            Tool::Measure => "Measure Tool",
            Tool::Area => "Area Tool",
        }
    }
    pub fn is_type(self) -> bool {
        matches!(self, Self::Type | Self::VerticalType | Self::ArtText | Self::FrameText | Self::TextOnPath)
    }

    /// Retouching and painting tools that stroke with the brush (share the brush cursor and chip).
    pub fn is_brushlike(self) -> bool {
        matches!(
            self,
            Tool::Brush
                | Tool::Pencil
                | Tool::MixerBrush
                | Tool::Eraser
                | Tool::BackgroundEraser
                | Tool::SpotHealing
                | Tool::Healing
                | Tool::CloneStamp
                | Tool::HistoryBrush
                | Tool::Blur
                | Tool::Sharpen
                | Tool::Smudge
                | Tool::Dodge
                | Tool::Burn
                | Tool::Sponge
                | Tool::AiRemove
                | Tool::AiCutout
        )
    }
    /// Photoshop default single-key shortcut (`'\0'` = none, e.g. the Blur group).
    pub fn key(self) -> char {
        match self {
            Tool::Move => 'V',
            Tool::RectMarquee | Tool::EllipseMarquee => 'M',
            Tool::Brush | Tool::Pencil | Tool::MixerBrush => 'B',
            Tool::Eraser | Tool::BackgroundEraser | Tool::MagicEraser => 'E',
            Tool::Eyedropper | Tool::Ruler | Tool::Note | Tool::Count => 'I',
            Tool::Lasso | Tool::PolygonLasso | Tool::MagneticLasso => 'L',
            Tool::MagicWand => 'W',
            Tool::Crop | Tool::PerspectiveCrop | Tool::Slice | Tool::SliceSelect => 'C',
            Tool::Gradient | Tool::PaintBucket => 'G',
            Tool::Type | Tool::VerticalType => 'T',
            Tool::Hand => 'H',
            Tool::RotateView => 'R',
            Tool::Zoom => 'Z',
            // Photoshop: Remove sits in the J healing group and R is Rotate View. A saved
            // `tools.select.AiRemove` binding still selects AI Remove directly.
            Tool::SpotHealing | Tool::Healing | Tool::Patch | Tool::ContentAwareMove | Tool::RedEye | Tool::AiRemove => 'J',
            Tool::AiCutout => 'K',
            Tool::CloneStamp => 'S',
            Tool::HistoryBrush => 'Y',
            Tool::Blur | Tool::Sharpen | Tool::Smudge => '\0',
            Tool::Dodge | Tool::Burn | Tool::Sponge => 'O',
            Tool::QuickSelection | Tool::ObjectSelection => 'W',
            Tool::Pen => 'P',
            Tool::PathSelection | Tool::DirectSelection => 'A',
            Tool::Rectangle | Tool::EllipseShape | Tool::Triangle | Tool::Polygon | Tool::Line | Tool::CustomShape => 'U',
            Tool::PointTransform => '\0',
            Tool::VectorPencil => '\0',
            Tool::VectorBrush => '\0',
            Tool::Smooth => '\0',
            Tool::PathEraser => '\0',
            Tool::Join => '\0',
            Tool::Corner => '\0',
            Tool::RoundedRectangle => '\0',
            Tool::Star => '\0',
            Tool::DoubleStar => '\0',
            Tool::SquareStar => '\0',
            Tool::Trapezoid => '\0',
            Tool::Diamond => '\0',
            Tool::Donut => '\0',
            Tool::Pie => '\0',
            Tool::Segment => '\0',
            Tool::Arrow => '\0',
            Tool::Callout => '\0',
            Tool::Cog => '\0',
            Tool::Heart => '\0',
            Tool::Tear => '\0',
            Tool::Crescent => '\0',
            Tool::Cloud => '\0',
            Tool::Spiral => '\0',
            Tool::Arc => '\0',
            Tool::RectGrid => '\0',
            Tool::PolarGrid => '\0',
            Tool::VectorFill => '\0',
            Tool::Transparency => '\0',
            Tool::ShapeBuilder => '\0',
            Tool::Contour => '\0',
            Tool::Knife => '\0',
            Tool::Scissors => '\0',
            Tool::LineCut => '\0',
            Tool::Width => '\0',
            Tool::ArtText => '\0',
            Tool::FrameText => '\0',
            Tool::TextOnPath => '\0',
            Tool::Blend => '\0',
            Tool::Repeat => '\0',
            Tool::Envelope => '\0',
            Tool::VectorCrop => '\0',
            Tool::Artboard => '\0',
            Tool::GroupSelection => '\0',
            Tool::VectorMagicWand => '\0',
            Tool::VectorLasso => '\0',
            Tool::Curvature => '\0',
            Tool::AddAnchor => '\0',
            Tool::DeleteAnchor => '\0',
            Tool::AnchorPoint => '\0',
            Tool::Rotate => '\0',
            Tool::Reflect => '\0',
            Tool::Scale => '\0',
            Tool::Shear => '\0',
            Tool::Reshape => '\0',
            Tool::FreeTransform => '\0',
            Tool::StylePicker => '\0',
            Tool::BlobBrush => '\0',
            Tool::VectorEraser => '\0',
            Tool::VectorWarp => '\0',
            Tool::VectorTwirl => '\0',
            Tool::VectorPucker => '\0',
            Tool::VectorBloat => '\0',
            Tool::Scallop => '\0',
            Tool::Crystallize => '\0',
            Tool::Wrinkle => '\0',
            Tool::VectorPuppetWarp => '\0',
            Tool::LivePaintBucket => '\0',
            Tool::VectorFloodFill => '\0',
            Tool::LivePaintSelection => '\0',
            Tool::Mesh => '\0',
            Tool::Measure => '\0',
            Tool::Area => '\0',
        }
    }
    /// Glyph drawn in the toolbar (vector icons come later).
    pub fn glyph(self) -> &'static str {
        match self {
            Tool::Move => "✥",
            Tool::RectMarquee => "⬚",
            Tool::EllipseMarquee => "◌",
            Tool::Brush => "🖌",
            Tool::MixerBrush => "🖌",
            Tool::Eraser => "⌫",
            Tool::Eyedropper => "💧",
            Tool::Lasso | Tool::PolygonLasso | Tool::MagneticLasso => "L",
            Tool::MagicWand => "W",
            Tool::Crop => "C",
            Tool::Gradient | Tool::PaintBucket => "G",
            Tool::Type | Tool::VerticalType => "T",
            Tool::Hand => "✋",
            Tool::Zoom => "🔍",
            _ => "•",
        }
    }
    pub fn personas(self) -> &'static [photocraft_engine::toolsets::Persona] {
        use photocraft_engine::toolsets::{Persona, supports};
        let name = format!("{self:?}");
        match (supports(&name, Persona::Pixel), supports(&name, Persona::Vector)) {
            (true, true) => &[Persona::Pixel, Persona::Vector],
            (true, false) => &[Persona::Pixel],
            (false, true) => &[Persona::Vector],
            _ => &[],
        }
    }
    pub fn from_name(s: &str) -> Option<Tool> {
        let n = s.to_ascii_lowercase().replace([' ', '_', '-'], "");
        Tool::ALL.into_iter().find(|t| {
            let a = format!("{t:?}").to_ascii_lowercase();
            n == a || n == a.replace("marquee", "") || n == t.label().to_ascii_lowercase().replace(' ', "")
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panels {
    pub layers: bool,
    pub history: bool,
    pub properties: bool,
    pub color: bool,
    pub navigator: bool,
    pub toolbar: bool,
    pub options_bar: bool,
    pub status_bar: bool,
    /// Window › Brush Settings (F5): floating, like Photoshop's.
    #[serde(default)]
    pub brush_settings: bool,
    /// Window › Character / Paragraph: the Character | Paragraph dock group (#150).
    #[serde(default)]
    pub character: bool,
    /// local-image: the Generate | Library dock group.
    #[serde(default = "yes")]
    pub generate: bool,
}

fn yes() -> bool {
    true
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            layers: true,
            history: false,
            properties: true,
            color: true,
            navigator: false,
            toolbar: true,
            options_bar: true,
            status_bar: true,
            brush_settings: false,
            character: false,
            // local-image: shown by Window › Generate, New from Prompt or the Home prompt.
            generate: false,
        }
    }
}

/// A modal or modeless dialog, identified by `id`, with editable fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dialog {
    pub id: u64,
    pub kind: DialogKind,
    /// Field values by name. Dialog widgets read and write these; so can automation.
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogKind {
    NewDocument,
    About,
    /// Parameter dialog for a command (generated from its params).
    Command,
    Error,
    /// Photoshop Layer Style dialog (see `layer_style.rs`).
    LayerStyle,
}

/// Per-document view (camera) state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    /// Screen pixels per document pixel.
    pub zoom: f32,
    /// Document-space point shown at the canvas centre.
    pub center: [f32; 2],
    /// Clockwise camera angle in radians; document geometry and exports stay unchanged.
    #[serde(default)]
    pub rotation: f32,
    /// An in-progress navigation gesture belongs to this view only.
    #[serde(skip)]
    pub rotation_drag: Option<crate::canvas::rotate_view::Drag>,
    /// Recompute fit-to-screen on next frame.
    pub fit_pending: bool,
    /// Document size this view last showed; a change (Image/Canvas Size, crop) re-centres it.
    #[serde(default)]
    pub doc_size: [u32; 2],
}

impl Default for View {
    fn default() -> Self {
        Self { zoom: 1.0, center: [0.0, 0.0], rotation: 0.0, rotation_drag: None, fit_pending: true, doc_size: [0, 0] }
    }
}

/// An extra OS window showing a document ("Window → Arrange → New Window for Document").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocWindow {
    pub id: u64,
    pub document: usize,
    pub view: View,
    pub open: bool,
}

/// Options-bar state for tools (Photoshop keeps these per tool).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolOptions {
    pub tolerance: f32,
    pub contiguous: bool,
    pub anti_alias: bool,
    pub sample_all_layers: bool,
    /// local-image: Magic Wand Sample Size (1 = point sample, else an N×N average).
    pub wand_sample: u32,
    pub feather: f32,
    /// linear | radial | angle | reflected | diamond
    pub gradient_style: String,
    pub gradient_reverse: bool,
    /// Gradient tool: dither to reduce 8-bit banding (Photoshop default on).
    pub gradient_dither: bool,
    /// Gradient tool mode: false = "Gradient" (live: a Gradient Fill layer, editable on canvas),
    /// true = "Classic gradient" (paints the pixels).
    pub gradient_classic: bool,
    /// Blend mode used by both live and classic gradient drags.
    pub gradient_blend_mode: photocraft_color::BlendMode,
    pub fill_opacity: f32,
    /// Paint Bucket fill source: false = Foreground colour, true = Pattern (Patterns panel selection).
    pub bucket_fill_pattern: bool,
    /// Type tool: family, style name, size (pt), anti-aliasing and paragraph alignment.
    pub type_font: String,
    pub type_style: String,
    pub type_size: f32,
    pub type_aa: String,
    pub type_align: String,
    /// Clone Stamp / Healing Brush.
    pub clone_aligned: bool,
    /// current | currentAndBelow | all
    pub clone_sample: String,
    /// local-image: Clone Stamp › Seamless (the clone blends into its surroundings).
    pub clone_seamless: bool,
    /// Spot Healing: contentAware | createTexture | proximityMatch
    pub spot_type: String,
    /// Patch: source (repair the selection) | destination (repair where it is dragged).
    pub patch_mode: String,
    /// Content-Aware Move: move | extend, Structure 1..7, Color 0..10.
    pub cam_mode: String,
    pub cam_structure: f32,
    pub cam_color: f32,
    /// Dodge/Burn: shadows | midtones | highlights, exposure %, protect tones.
    pub tone_range: String,
    pub exposure: f32,
    pub protect_tones: bool,
    /// Sponge: desaturate | saturate, vibrance.
    pub sponge_mode: String,
    pub vibrance: bool,
    /// Blur/Sharpen/Smudge strength %, Sharpen protect detail, Smudge finger painting.
    pub strength: f32,
    pub protect_detail: bool,
    pub finger_painting: bool,
    /// Quick Selection enhance edge.
    pub enhance_edge: bool,
    /// local-image: Quick Selection › Subject Assist (the installed selection model as a prior).
    pub quick_subject_assist: bool,
    /// Pen: "path" (work path) or "shape" (shape layer).
    pub vector_mode: String,
    /// Shape tools: fill with the foreground colour, stroke width (0 = none), rectangle corner
    /// radius, polygon sides, line weight.
    pub shape_fill: bool,
    #[serde(default)]
    pub pen_options: crate::vector_packages::pen::Options,
    pub stroke_width: f32,
    pub corner_radius: f32,
    pub polygon_sides: u32,
    pub line_weight: f32,
    /// Marquee options bar: "normal" | "fixedRatio" | "fixedSize", with the ratio or size.
    #[serde(default = "default_marquee_style")]
    pub marquee_style: String,
    #[serde(default = "one")]
    pub marquee_width: f32,
    #[serde(default = "one")]
    pub marquee_height: f32,
    /// Move tool: Auto-Select (with "layer" or "group" target) and Show Transform Controls.
    /// Auto-Select is on by default, as in current Photoshop; ⌘/Ctrl-click inverts it.
    #[serde(default = "yes")]
    pub move_auto_select: bool,
    #[serde(default = "default_move_target")]
    pub move_target: String,
    #[serde(default)]
    pub move_show_transform: bool,
    /// Crop options bar: aspect ratio ("" = Ratio/unconstrained, "w:h", or "original") and
    /// Delete Cropped Pixels.
    #[serde(default)]
    pub crop_ratio: String,
    #[serde(default = "yes")]
    pub crop_delete: bool,
    /// local-image: the crop frame's composition overlay: `thirds`, `grid`, `diagonal`,
    /// `triangle`, `golden` or `none`.
    pub crop_overlay: String,
    /// local-image: Crop › Straighten: the frame's clockwise tilt on the image, in degrees.
    pub crop_angle: f32,
    /// local-image: Red Eye tool options (%).
    pub red_eye_pupil: f32,
    pub red_eye_darken: f32,
    /// local-image: Pet Eye (Lightroom's: a pet's glowing pupil) and its Add Catchlight.
    pub red_eye_pet: bool,
    pub red_eye_catchlight: bool,
    /// Magic Eraser opacity % (tolerance, anti-alias, contiguous and sample-all are shared with the
    /// Magic Wand and Paint Bucket).
    pub magic_eraser_opacity: f32,
    /// Background Eraser: sampling (continuous | once | backgroundSwatch), limits (discontiguous |
    /// contiguous | findEdges), tolerance % and Protect Foreground Color.
    pub bg_sampling: String,
    pub bg_limits: String,
    pub bg_tolerance: f32,
    pub bg_protect_fg: bool,
    /// Zoom tool › Scrubby Zoom: dragging left/right zooms continuously (else a zoom rectangle).
    #[serde(default = "yes")]
    pub zoom_scrubby: bool,
    /// Pencil › Auto Erase: a stroke that starts on the foreground colour paints the background colour.
    #[serde(default)]
    pub pencil_auto_erase: bool,
    /// Magnetic Lasso: detection width (px, 1..256), edge contrast (%, 1..100), how often it
    /// fastens points by itself (0..100), and whether pen pressure narrows the width.
    pub magnetic_width: f32,
    pub magnetic_contrast: f32,
    pub magnetic_frequency: f32,
    pub magnetic_pressure: bool,
}

fn default_move_target() -> String {
    "layer".into()
}

fn default_marquee_style() -> String {
    "normal".into()
}

fn one() -> f32 {
    1.0
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            tolerance: 32.0,
            contiguous: true,
            anti_alias: true,
            sample_all_layers: false,
            wand_sample: 1,
            feather: 0.0,
            gradient_style: "linear".into(),
            gradient_reverse: false,
            gradient_dither: true,
            gradient_classic: false,
            gradient_blend_mode: photocraft_color::BlendMode::Normal,
            fill_opacity: 100.0,
            bucket_fill_pattern: false,
            type_font: "Inter".into(),
            type_style: "Regular".into(),
            type_size: 48.0,
            type_aa: "sharp".into(),
            type_align: "left".into(),
            clone_aligned: true,
            clone_sample: "current".into(),
            clone_seamless: false,
            spot_type: "contentAware".into(),
            patch_mode: "source".into(),
            cam_mode: "move".into(),
            cam_structure: 4.0,
            cam_color: 0.0,
            tone_range: "midtones".into(),
            exposure: 50.0,
            protect_tones: true,
            sponge_mode: "desaturate".into(),
            vibrance: true,
            strength: 50.0,
            protect_detail: true,
            finger_painting: false,
            enhance_edge: false,
            quick_subject_assist: true,
            vector_mode: "path".into(),
            shape_fill: true,
            pen_options: Default::default(),
            stroke_width: 0.0,
            corner_radius: 0.0,
            polygon_sides: 5,
            line_weight: 3.0,
            marquee_style: default_marquee_style(),
            marquee_width: 1.0,
            marquee_height: 1.0,
            move_auto_select: true,
            move_target: default_move_target(),
            move_show_transform: false,
            crop_ratio: String::new(),
            crop_delete: true,
            crop_overlay: "thirds".into(),
            crop_angle: 0.0,
            red_eye_pupil: 50.0,
            red_eye_darken: 50.0,
            red_eye_pet: false,
            red_eye_catchlight: false,
            magic_eraser_opacity: 100.0,
            bg_sampling: "continuous".into(),
            bg_limits: "contiguous".into(),
            bg_tolerance: 50.0,
            bg_protect_fg: false,
            zoom_scrubby: true,
            pencil_auto_erase: false,
            magnetic_width: 10.0,
            magnetic_contrast: 10.0,
            magnetic_frequency: 57.0,
            magnetic_pressure: false,
        }
    }
}

/// Edit › Transform's mode: what a handle drag does with no modifier keys held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransformMode {
    /// Free Transform, Scale and Rotate.
    #[default]
    Free,
    /// Edge handles skew.
    Skew,
    /// Corner handles move freely, one at a time; nothing else moves them (no rotating, no
    /// edges) and nothing snaps.
    Distort,
    /// Corner handles move in pairs, mirrored (one-point perspective).
    Perspective,
}

impl TransformMode {
    /// The mode an `edit.transform.*` / `edit.freeTransform` menu id starts.
    pub fn for_command(id: &str) -> Self {
        match id {
            "edit.transform.skew" => Self::Skew,
            "edit.transform.distort" => Self::Distort,
            "edit.transform.perspective" => Self::Perspective,
            _ => Self::Free,
        }
    }
}

/// Free Transform in progress: the source frame `rect` and where its corners currently are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformSession {
    pub session: u64,
    pub layer: u64,
    pub rect: [f64; 4],
    /// Corners clockwise from top-left (document px).
    pub quad: [[f64; 2]; 4],
    /// Reference point (rotation / ⌥-scale centre).
    pub pivot: [f64; 2],
    pub interpolation: String,
    /// Warp mode (Edit › Transform › Warp): the warp being edited over `rect`, in document px.
    #[serde(default)]
    pub warp: Option<photocraft_geom::warp::Warp>,
    /// Select › Transform Selection: the box transforms the selection outline, not pixels.
    #[serde(default)]
    pub selection: bool,
    /// `edit.transform`'s `"target"` when the box moves an unlinked layer mask, an alpha channel or
    /// the Quick Mask by itself (`None`: the layer, with its linked masks).
    #[serde(default)]
    pub target: Option<serde_json::Value>,
    /// The layer was made for this session (⌥⌘T's copy, #352; a file dropped on the canvas), so
    /// Cancel takes it back and OK folds it into one history step with the transform.
    #[serde(default)]
    pub made: Option<MadeLayer>,
    /// Edit › Transform › Skew / Distort / Perspective (`Free` for Free Transform).
    #[serde(default)]
    pub mode: TransformMode,
}

/// Why a Free Transform session's layer was made for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MadeLayer {
    /// Free Transform on a copy (⌥⌘T): OK makes the copy and the transform one Free Transform step.
    Copy,
    /// A file dropped on the canvas: OK makes the place and the transform one Place Embedded step.
    Place,
}

/// In-progress inline type editing (Type tool). Offsets are character indices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextEdit {
    pub layer: u64,
    pub caret: usize,
    pub anchor: usize,
    /// History coalescing key: the whole editing session is one "Edit Type Layer" step.
    pub session: String,
    /// The layer was created by this session (its name follows the text; empty on commit = delete).
    pub created: bool,
    #[serde(skip)]
    pub dragging: bool,
    /// Paragraph-box handle being dragged (0-3 corners from top-left clockwise, 4-7 top/right/bottom/left edges).
    #[serde(skip)]
    pub resize: Option<u8>,
    /// IME composition in progress: (start, length) in characters. The preedit text lives in the
    /// layer so it lays out like typed text; each IME update replaces it.
    #[serde(skip)]
    pub preedit: Option<(usize, usize)>,
}

/// View-menu overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Extras {
    pub rulers: bool,
    pub grid: bool,
    pub guides: bool,
    pub snap: bool,
    pub lock_guides: bool,
}

impl Default for Extras {
    fn default() -> Self {
        Self { rulers: false, grid: false, guides: true, snap: true, lock_guides: false }
    }
}

/// Selected tab per dock card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockTabs {
    pub properties: usize,
    pub color: usize,
    pub layers: usize,
    /// Navigator | Histogram | Info.
    pub navigator: usize,
    /// History | Actions.
    pub history: usize,
    /// Character | Paragraph.
    pub character: usize,
    /// local-image: Generate | Library.
    pub generate: usize,
}

/// Color panel state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorPanelState {
    /// The panel edits the background colour (its chip was clicked), not the foreground.
    pub background: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiState {
    #[serde(default)]
    pub symbols: crate::symbols::State,
    #[serde(skip)]
    pub persona_pointer: Runtime<crate::toolsets_ui::persona::PointerGuard>,
    /// Successful photo admission awaiting a frame with this document still active.
    #[serde(skip)]
    pub photo_admission: Option<photocraft_doc::DocId>,
    #[serde(skip)]
    pub vector_blend: Runtime<crate::blend::State>,
    #[serde(skip)]
    pub svg_export: Runtime<crate::vector_packages::svg_export::State>,
    #[serde(skip)]
    pub pdf_export: Runtime<crate::vector_packages::pdf_export::State>,
    #[serde(skip)]
    pub frame_compose: Runtime<crate::frame_compose::State>,
    #[serde(skip)]
    pub fill_stroke_proxy: Runtime<crate::fill_stroke_proxy::State>,
    pub tool: Tool,
    /// local-image: AI tool options and the Generate panel (ai_ui.rs, generate_ui.rs).
    #[serde(default)]
    pub ai: crate::ai_ui::AiOptions,
    /// Recently opened file paths, most-recent first (File › Open Recent). Capped; de-duplicated.
    #[serde(default)]
    pub recent_files: Vec<String>,
    /// Inline type editing session, if any.
    #[serde(default)]
    pub text_edit: Option<TextEdit>,
    /// Clock time in seconds when the type caret's blink was last reset (a keystroke, a click that
    /// places the caret, a drag): the caret is solid for a blink period after it.
    #[serde(skip)]
    pub caret_blink_time: f64,
    /// Free Transform session, if any.
    #[serde(default)]
    pub transform: Option<TransformSession>,
    /// Clone Stamp / Healing source point (⌥-click) and the aligned offset once a stroke started.
    #[serde(default)]
    pub clone_source: Option<[f64; 2]>,
    #[serde(default)]
    pub clone_offset: Option<[f64; 2]>,
    /// Painting targets the active layer's mask instead of its pixels.
    #[serde(default)]
    pub mask_target: bool,
    /// The active layer's vector mask is targeted (its Layers thumbnail is bracketed): the path
    /// tools edit it (#196). Never set together with `mask_target`.
    #[serde(default)]
    pub vector_mask_target: bool,
    /// Brush Preset picker opened by a right-click on the canvas: its screen position (points).
    #[serde(default)]
    pub brush_picker: Option<[f32; 2]>,
    /// Layers under the pointer, listed by a right-click on the canvas with the Move tool or
    /// ⌘/Ctrl+right-click with any tool (`layer_pick_ui`, #307).
    #[serde(default)]
    pub layer_menu: Option<crate::layer_pick_ui::LayerMenu>,
    /// Selection-tool context menu opened by a plain canvas right-click.
    #[serde(default)]
    pub canvas_tool_menu: Option<crate::canvas_tool_menu::CanvasToolMenu>,
    /// Smoothing is a per-tool option (Brush and Eraser each keep theirs): the tool whose
    /// smoothing the session brush holds, and the other tools' saved values.
    #[serde(default)]
    pub smoothing_tool: Option<Tool>,
    #[serde(default)]
    pub tool_smoothing: Vec<(Tool, photocraft_engine::paint::brush::Smoothing)>,
    /// Pen path under construction.
    #[serde(default)]
    pub pen: Option<crate::vector_ui::PenPath>,
    /// Direct Selection tool: selected anchors and the drag in progress (#790).
    #[serde(default)]
    pub direct_selection: crate::direct_select::DirectSelection,
    /// Selected row in the Paths panel ("work" or a saved path name).
    #[serde(default)]
    pub selected_path: Option<String>,
    /// Layers panel kind filter ("pixel", "adjustment", "type", "shape", "smart"); empty = all.
    #[serde(default)]
    pub layer_filter: Vec<String>,
    /// Actions panel: which row is selected and which are expanded. The list lives on the session.
    #[serde(default)]
    pub actions: crate::actions::ActionsUi,
    /// Layer Comps panel: the selected comp (by comp id).
    #[serde(default)]
    pub layer_comp_selected: Option<u32>,
    /// Preset panels (Gradients, Patterns, Styles, Shapes, Tool Presets, Clone Source).
    #[serde(default)]
    pub presets_ui: crate::preset_panels::PresetUi,
    /// Character/Paragraph Styles, Glyphs and Check Spelling (see `type_panels_ui`).
    #[serde(default)]
    pub type_panels: crate::type_panels_ui::TypePanelsUi,
    /// Ruler/Count/Note tools, Measurement Log and Notes panels (see `analysis_ui`).
    #[serde(default)]
    pub analysis: crate::analysis_ui::AnalysisUi,
    pub timeline: crate::timeline_ui::TimelineUi,
    /// Slice and Slice Select tools (see `slice_ui`).
    #[serde(default)]
    pub slices: crate::slice_ui::SliceUi,
    /// Modifier Keys panel, custom pixel aspect ratios, workspace dialogs (see `workspace_ui`).
    #[serde(default)]
    pub shell: crate::workspace_ui::ShellUi,
    /// View extras: rulers (⌘R), grid (⌘'), guides (⌘;), snapping (⇧⌘;), locked guides (⌥⌘;).
    #[serde(default)]
    pub extras: Extras,
    /// View / Window / Type preferences: screen mode, Extras, Show and Snap To, flip, arrangement.
    #[serde(default)]
    pub view: crate::view_cmds::ViewOptions,
    pub panels: Panels,
    /// Views per open document (index-aligned with the session's documents).
    pub views: Vec<View>,
    pub dialogs: Vec<Dialog>,
    pub windows: Vec<DocWindow>,
    pub theme: crate::theme::ThemeKind,
    pub workspace: String,
    pub palette_open: bool,
    pub dock_tabs: DockTabs,
    /// Right-dock group order, heights and collapsed groups (see `dock`).
    #[serde(default)]
    pub dock: crate::dock::DockLayout,
    /// Which chip the Color panel edits.
    #[serde(default)]
    pub color_panel: ColorPanelState,
    /// Brush Settings: selected section (0 = Brush Tip Shape) and tab (0 settings, 1 Brushes).
    #[serde(default)]
    pub brush_section: usize,
    #[serde(default)]
    pub brush_tab: usize,
    /// Brushes panel: collapsed groups and the search filter.
    #[serde(default)]
    pub brushes_panel: crate::brush_panel::BrushesPanelState,
    /// Marquee options-bar mode: 0 new, 1 add, 2 subtract, 3 intersect (modifier keys override).
    #[serde(default)]
    pub selection_mode: u8,
    #[serde(default)]
    pub tool_options: ToolOptions,
    #[serde(default)]
    pub trace_preferences: crate::trace_dialog::Preferences,
    #[serde(default)]
    pub vector_snapping: photocraft_engine::vector_packages::snapping::query::Settings,
    /// In-progress polygonal lasso vertices (document coordinates).
    #[serde(default)]
    pub polygon: Vec<[f64; 2]>,
    /// The selection mode the polygonal lasso started in ("replace", "add", ...), set by the
    /// modifiers held at its first click.
    #[serde(default)]
    pub polygon_mode: String,
    /// The document the polygon is drawn on (set at its first click): another document becoming
    /// active drops it (`canvas::polygon_cancel_stale`).
    #[serde(default)]
    pub polygon_doc: Option<photocraft_doc::DocId>,
    /// Magnetic Lasso border in progress (`magnetic_lasso_ui`).
    #[serde(default)]
    pub magnetic: crate::magnetic_lasso_ui::MagneticLasso,
    /// Crop tool rectangle being edited [x0, y0, x1, y1] (document coordinates).
    #[serde(default)]
    pub crop_rect: Option<[f64; 4]>,
    pub next_id: u64,
    /// Last status message (errors from commands, hints).
    pub status: String,
    /// The status message is an error (shown in the warning colour).
    #[serde(default)]
    pub status_error: bool,
    /// Non-blocking notices (import/export warnings, files that couldn't open), newest last.
    #[serde(default)]
    pub notices: Vec<li_ui_kit::notices::Notice>,
    /// Recent complete operation results, including dismissed/compacted notices.
    #[serde(default)]
    pub notice_history: Vec<li_ui_kit::notices::Notice>,
    #[serde(default)]
    pub notice_history_omitted: usize,
    #[serde(skip)]
    pub notice_history_open: bool,
    /// Pending GPU fallback warning, visible to automation.
    #[serde(default)]
    pub gpu_fallback_notice: Option<String>,
    /// Status bar info field, Home screen (see `chrome_ui`).
    #[serde(default)]
    pub chrome: crate::chrome_ui::ChromeState,
    #[serde(default)]
    pub camera_raw_scope: CameraRawScopeState,
    #[serde(default)]
    pub camera_raw_preview: CameraRawPreviewState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            symbols: Default::default(),
            persona_pointer: Default::default(),
            photo_admission: None,
            vector_blend: Default::default(),
            svg_export: Default::default(),
            pdf_export: Default::default(),
            frame_compose: Default::default(),
            fill_stroke_proxy: Default::default(),
            tool: Tool::Move,
            ai: Default::default(),
            recent_files: Vec::new(),
            text_edit: None,
            caret_blink_time: 0.0,
            transform: None,
            mask_target: false,
            vector_mask_target: false,
            brush_picker: None,
            layer_menu: None,
            canvas_tool_menu: None,
            smoothing_tool: None,
            tool_smoothing: Vec::new(),
            clone_source: None,
            clone_offset: None,
            extras: Extras::default(),
            view: Default::default(),
            actions: Default::default(),
            layer_comp_selected: None,
            presets_ui: Default::default(),
            type_panels: Default::default(),
            analysis: Default::default(),
            timeline: Default::default(),
            slices: Default::default(),
            shell: Default::default(),
            layer_filter: Vec::new(),
            pen: None,
            direct_selection: Default::default(),
            selected_path: None,
            panels: Panels::default(),
            views: Vec::new(),
            dialogs: Vec::new(),
            windows: Vec::new(),
            theme: crate::theme::ThemeKind::ProMedium,
            workspace: "Essentials".into(),
            palette_open: false,
            dock_tabs: DockTabs::default(),
            dock: Default::default(),
            color_panel: Default::default(),
            brush_section: 0,
            brush_tab: 0,
            brushes_panel: Default::default(),
            selection_mode: 0,
            tool_options: ToolOptions::default(),
            trace_preferences: Default::default(),
            vector_snapping: Default::default(),
            polygon: Vec::new(),
            polygon_mode: String::new(),
            polygon_doc: None,
            magnetic: Default::default(),
            crop_rect: None,
            next_id: 1,
            status: String::new(),
            status_error: false,
            notices: Vec::new(),
            notice_history: Vec::new(),
            notice_history_omitted: 0,
            notice_history_open: false,
            gpu_fallback_notice: None,
            chrome: Default::default(),
            camera_raw_scope: Default::default(),
            camera_raw_preview: Default::default(),
        }
    }
}

impl UiState {
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn open_dialog(&mut self, kind: DialogKind, fields: serde_json::Map<String, serde_json::Value>) -> u64 {
        let id = self.alloc_id();
        self.dialogs.push(Dialog { id, kind, fields });
        id
    }

    pub fn dialog_mut(&mut self, id: u64) -> Option<&mut Dialog> {
        self.dialogs.iter_mut().find(|d| d.id == id)
    }

    pub fn close_dialog(&mut self, id: u64) -> Option<Dialog> {
        let i = self.dialogs.iter().position(|d| d.id == id)?;
        Some(self.dialogs.remove(i))
    }

    pub fn new_document_fields() -> serde_json::Map<String, serde_json::Value> {
        let v = serde_json::json!({"name": "Untitled-1", "width": 1920, "height": 1080, "resolution": 72, "mode": "rgb", "depth": 8, "background": "white"});
        v.as_object().cloned().unwrap_or_default()
    }
}

/// Transient runtime excluded from persistent UI snapshots and their equality.
pub struct Runtime<T: Default>(pub T);
impl<T: Default> Default for Runtime<T> {
    fn default() -> Self {
        Self(T::default())
    }
}
impl<T: Default> Clone for Runtime<T> {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl<T: Default> std::fmt::Debug for Runtime<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Runtime")
    }
}
impl<T: Default> PartialEq for Runtime<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T: Default> std::ops::Deref for Runtime<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Default> std::ops::DerefMut for Runtime<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_parse() {
        assert_eq!(Tool::from_name("brush"), Some(Tool::Brush));
        assert_eq!(Tool::from_name("Rect"), Some(Tool::RectMarquee));
        assert_eq!(Tool::from_name("RectMarquee"), Some(Tool::RectMarquee));
        assert_eq!(Tool::from_name("Eraser Tool"), Some(Tool::Eraser));
        assert_eq!(Tool::from_name("mixerBrush"), Some(Tool::MixerBrush));
        assert_eq!(Tool::from_name("Mixer Brush Tool"), Some(Tool::MixerBrush));
        assert_eq!(Tool::from_name("nope"), None);
    }

    #[test]
    fn dialogs_open_and_close() {
        let mut s = UiState::default();
        let a = s.open_dialog(DialogKind::About, Default::default());
        let b = s.open_dialog(DialogKind::NewDocument, UiState::new_document_fields());
        assert_ne!(a, b);
        assert_eq!(s.dialog_mut(b).unwrap().fields["width"], 1920);
        assert!(s.close_dialog(a).is_some());
        assert_eq!(s.dialogs.len(), 1);
    }

    #[test]
    fn ui_state_serializes() {
        let s = UiState::default();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["tool"], "Move");
        let back: UiState = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }
}
