//! Persistent preferences (Edit › Preferences), keyboard-shortcut, menu and toolbar
//! customisation, grouped like Photoshop's Preferences dialog sections.
//!
//! The whole set is plain serde data owned by the [`Session`] ([`Session::prefs`]), so every
//! frontend reads the same values and agents change them with `prefs.get` / `prefs.set` /
//! `prefs.reset`. Sections that only the GUI shell honours (cursors, checkerboard, guide colours,
//! interface theme…) live here too: keeping one document means the headless MCP server sees and
//! edits exactly what the desktop app persists, and the shell has no second store to keep in sync.
//!
//! The engine never touches the filesystem for preferences (it must build for wasm). Frontends
//! persist [`Session::prefs_to_json`] wherever the platform keeps settings (the desktop app: the
//! OS config directory; the web shell: browser storage) and restore it with
//! [`Session::load_prefs_json`].
//!
//! Engine-side values are applied by [`Session::apply_prefs`]: the history state limit (every
//! open document), the effect-cache memory budget, and units for commands that report lengths.
//! Colour settings (`colorSettings`) are stored with the colour state
//! ([`crate::color_cmds::ColorSettings`]) and persisted in the same document.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

/// Load valid preference fields independently. One obsolete enum or malformed nested field
/// uses its default without throwing away neighbouring values. Runtime commands remain strict.
pub fn lenient<T: Serialize + serde::de::DeserializeOwned + Default>(input: Value) -> std::result::Result<(T, Vec<String>), serde_json::Error> {
    fn merge<T: serde::de::DeserializeOwned>(base: &mut Value, input: &Value, path: &mut Vec<String>, rejected: &mut Vec<String>) {
        fn at<'a>(v: &'a mut Value, path: &[String]) -> &'a mut Value {
            let mut v = v;
            for key in path {
                v = &mut v[key];
            }
            v
        }
        let old = at(base, path).clone();
        *at(base, path) = input.clone();
        if serde_json::from_value::<T>(base.clone()).is_ok() {
            return;
        }
        *at(base, path) = old.clone();
        if let (Value::Object(_), Value::Object(fields)) = (&old, input) {
            for (key, value) in fields {
                path.push(key.clone());
                merge::<T>(base, value, path, rejected);
                path.pop();
            }
        } else {
            rejected.push(path.join("."));
        }
    }
    let mut base = serde_json::to_value(T::default())?;
    let mut rejected = Vec::new();
    merge::<T>(&mut base, &input, &mut Vec::new(), &mut rejected);
    Ok((serde_json::from_value(base)?, rejected))
}

/// A string-valued choice enum with its JSON names (used to validate `prefs.set` and to fill
/// dropdowns in the Preferences dialog).
macro_rules! choice {
    ($(#[$m:meta])* $name:ident { $($var:ident = $s:literal),+ $(,)? } default $def:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $s)] $var),+ }
        impl Default for $name {
            fn default() -> Self { Self::$def }
        }
        impl $name {
            pub const NAMES: &'static [&'static str] = &[$($s),+];
            pub fn name(self) -> &'static str {
                match self { $(Self::$var => $s),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$var),)+ _ => None }
            }
        }
    };
}

choice!(
    /// Length units for rulers, the Info panel and size dialogs.
    Unit { Pixels = "pixels", Inches = "inches", Centimeters = "cm", Millimeters = "mm", Points = "points", Picas = "picas", Percent = "percent" } default Pixels
);
choice!(TypeUnit { Points = "points", Pixels = "pixels", Millimeters = "mm" } default Points);
choice!(PointSize { PostScript = "postScript", Traditional = "traditional" } default PostScript);
choice!(Interpolation { BicubicAutomatic = "bicubicAutomatic", Nearest = "nearestNeighbor", Bilinear = "bilinear", Bicubic = "bicubic", BicubicSmoother = "bicubicSmoother", BicubicSharper = "bicubicSharper", PreserveDetails = "preserveDetails" } default BicubicAutomatic);
choice!(ColorPicker { Adobe = "adobe", System = "system" } default Adobe);
choice!(Theme { Pro = "pro", ProMedium = "proMedium", Studio = "studio", StudioLight = "studioLight", Classic = "classic" } default ProMedium);
choice!(CanvasColor { Default = "default", Black = "black", DarkGray = "darkGray", MediumGray = "mediumGray", LightGray = "lightGray", Custom = "custom" } default Default);
choice!(CanvasBorder { DropShadow = "dropShadow", Line = "line", None = "none" } default DropShadow);
choice!(UiScale { Auto = "auto", P75 = "75", P100 = "100", P125 = "125", P150 = "150", P175 = "175", P200 = "200", P250 = "250", P300 = "300" } default Auto);
choice!(
    /// Graphics backend of the desktop app's window and GPU canvas (applies at next launch).
    /// `auto` lets PhotoCraft pick (DX12 for Intel adapters on Windows); `cpu` composites on the
    /// CPU and draws the window with a software adapter where the platform has one. A start that
    /// crashes inside the graphics driver moves this to the next safer choice.
    GpuBackend { Auto = "auto", Vulkan = "vulkan", Dx12 = "dx12", Metal = "metal", Gl = "gl", Cpu = "cpu" } default Auto
);
choice!(
    /// Rendering policy, independent of the advanced graphics backend selection.
    /// CPU disables image acceleration; the native window may still need hardware graphics.
    RenderingMode { Auto = "auto", Gpu = "gpu", Cpu = "cpu" } default Auto
);
choice!(
    /// local-image: OS priority of batch work (export, previews, indexing). `low` lets the UI win
    /// contested CPU time; `normal` runs it at full speed alongside the UI.
    BackgroundPriority { Low = "low", Normal = "normal" } default Low
);
choice!(
    /// Toolbar hover tips: `rich` (name, shortcut, what it does and how to use it), `simple` (name and
    /// shortcut) or `off`.
    ToolTips { Rich = "rich", Simple = "simple", Off = "off" } default Rich
);
choice!(UiFontSize { Tiny = "tiny", Small = "small", Medium = "medium", Large = "large" } default Small);
choice!(LogDestination { Metadata = "metadata", TextFile = "textFile", Both = "both" } default Metadata);
choice!(LogDetail { SessionsOnly = "sessionsOnly", Concise = "concise", Detailed = "detailed" } default Concise);
choice!(Ask { Always = "always", Never = "never", Ask = "ask" } default Always);
choice!(QuickExportFormat { Png = "png", Jpg = "jpg", Gif = "gif", Webp = "webp" } default Png);
choice!(ExportLocation { Ask = "ask", SameFolder = "sameFolder" } default Ask);
choice!(ExportMetadata { None = "none", Copyright = "copyright", All = "all" } default Copyright);
choice!(
    /// Painting cursors: Standard (tool icon), Precise (crosshair), Normal Brush Tip (the 50%
    /// opacity contour), Full Size Brush Tip (the whole diameter).
    PaintingCursor { Standard = "standard", Precise = "precise", NormalTip = "normalTip", FullSizeTip = "fullSizeTip" } default NormalTip
);
choice!(
    /// Right mouse button on the canvas with a painting tool: open the Brush Preset picker at the
    /// pointer (Photoshop), or erase with the current brush while dragging (Krita, Paint).
    RightClickPaint { BrushPicker = "brushPicker", Erase = "erase" } default BrushPicker
);
choice!(OtherCursor { Standard = "standard", Precise = "precise" } default Standard);
choice!(CheckerSize { None = "none", Small = "small", Medium = "medium", Large = "large" } default Medium);
choice!(CheckerColors { Light = "light", Medium = "medium", Dark = "dark", Red = "red", Orange = "orange", Green = "green", Blue = "blue", Purple = "purple", Custom = "custom" } default Light);
choice!(LineStyle { Lines = "lines", Dashed = "dashedLines", Dots = "dots" } default Lines);
choice!(TextEngine { Modern = "modern", EastAsian = "eastAsian", WorldReady = "worldReady" } default WorldReady);
choice!(FontPreview { Off = "off", Small = "small", Medium = "medium", Large = "large", ExtraLarge = "extraLarge", Huge = "huge" } default Medium);
choice!(RawColorSpace { Srgb = "srgb", AdobeRgb = "adobeRgb", ProPhoto = "proPhoto", DisplayP3 = "displayP3" } default AdobeRgb);
choice!(RawDepth { Eight = "8", Sixteen = "16" } default Sixteen);
choice!(RawSharpen { None = "none", Screen = "screen", Glossy = "glossy", Matte = "matte" } default None);

impl Unit {
    /// Document pixels → this unit. `dpi` is the document resolution, `extent` the 100% length
    /// (for percent), `ppi` points per inch (72 PostScript, 72.27 traditional).
    pub fn from_px(self, px: f64, dpi: f64, extent: f64, ppi: f64) -> f64 {
        let dpi = dpi.max(1e-6);
        match self {
            Unit::Pixels => px,
            Unit::Inches => px / dpi,
            Unit::Centimeters => px / dpi * 2.54,
            Unit::Millimeters => px / dpi * 25.4,
            Unit::Points => px / dpi * ppi,
            Unit::Picas => px / dpi * ppi / 12.0,
            Unit::Percent => px / extent.max(1e-6) * 100.0,
        }
    }
    /// This unit → document pixels (inverse of [`Unit::from_px`]).
    pub fn to_px(self, v: f64, dpi: f64, extent: f64, ppi: f64) -> f64 {
        let dpi = dpi.max(1e-6);
        match self {
            Unit::Pixels => v,
            Unit::Inches => v * dpi,
            Unit::Centimeters => v / 2.54 * dpi,
            Unit::Millimeters => v / 25.4 * dpi,
            Unit::Points => v / ppi.max(1e-6) * dpi,
            Unit::Picas => v * 12.0 / ppi.max(1e-6) * dpi,
            Unit::Percent => v * extent / 100.0,
        }
    }
    /// Short suffix for readouts ("px", "in", "cm", …).
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Pixels => "px",
            Unit::Inches => "in",
            Unit::Centimeters => "cm",
            Unit::Millimeters => "mm",
            Unit::Points => "pt",
            Unit::Picas => "pica",
            Unit::Percent => "%",
        }
    }
    /// Decimal places a readout in this unit needs.
    pub fn decimals(self) -> usize {
        match self {
            Unit::Pixels => 0,
            Unit::Points | Unit::Percent | Unit::Millimeters => 1,
            _ => 2,
        }
    }
}

impl PointSize {
    /// Points per inch.
    pub fn per_inch(self) -> f64 {
        match self {
            PointSize::PostScript => 72.0,
            PointSize::Traditional => 72.27,
        }
    }
}

// ------------------------------------------------------------------ sections

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    pub color_picker: ColorPicker,
    /// Default resampling for Image Size and transforms.
    pub image_interpolation: Interpolation,
    pub beep_when_done: bool,
    pub export_clipboard: bool,
    pub resize_image_during_place: bool,
    pub always_create_smart_objects_when_placing: bool,
    /// Scroll wheel zooms instead of panning.
    pub zoom_with_scroll_wheel: bool,
    pub animated_zoom: bool,
    pub zoom_resizes_windows: bool,
    pub use_legacy_free_transform: bool,
    pub auto_show_home_screen: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            color_picker: ColorPicker::Adobe,
            image_interpolation: Interpolation::BicubicAutomatic,
            beep_when_done: false,
            export_clipboard: true,
            resize_image_during_place: true,
            always_create_smart_objects_when_placing: true,
            zoom_with_scroll_wheel: false,
            animated_zoom: true,
            zoom_resizes_windows: false,
            use_legacy_free_transform: false,
            auto_show_home_screen: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Interface {
    pub theme: Theme,
    /// Original colour tools, or the existing theme-tinted monochrome glyphs.
    pub tool_icons: String,
    /// Pasteboard colour in standard screen mode (`canvasCustomColor` when "custom").
    pub canvas_color: CanvasColor,
    pub canvas_custom_color: String,
    /// Document border in standard screen mode.
    pub canvas_border: CanvasBorder,
    pub ui_scale: UiScale,
    /// UI language: `auto` (follow the system) or a language code such as `en`, `ja`. The list of
    /// languages belongs to the shell (`ui-egui` i18n); an unknown code falls back to `auto`.
    /// Command ids and the control protocol stay English.
    pub language: String,
    pub ui_font_size: UiFontSize,
    pub show_channels_in_color: bool,
    pub dynamic_color_sliders: bool,
    /// Draw menu item colours set with Edit › Menus.
    pub show_menu_colors: bool,
    pub show_tooltips: bool,
    /// local-image: how much a tool button's hover tip shows.
    pub tool_tips: ToolTips,
    /// Move tool drags show only the layer's outline and an arrow, leaving its pixels in place
    /// until release. Off (the default), the pixels follow the pointer live inside the outline.
    pub show_bounding_box_when_dragging_layer: bool,
    /// local-image: the Contextual Task Bar under a new selection or closed path (Window ›
    /// Contextual Task Bar).
    pub contextual_task_bar: bool,
    /// local-image: opening a file shows the rest of its folder in the filmstrip.
    pub filmstrip_shows_folder: bool,
    /// local-image: where the filmstrip's Save & Next writes an edited image: `editedFolder` (a
    /// copy in an `Edited` folder beside the originals), `original` (over the original file) or
    /// `saveAs` (Save As).
    pub save_and_next_saves_to: String,
}

impl Default for Interface {
    fn default() -> Self {
        Self {
            theme: Theme::ProMedium,
            tool_icons: "monochrome".into(),
            canvas_color: CanvasColor::Default,
            canvas_custom_color: "#282828".into(),
            canvas_border: CanvasBorder::DropShadow,
            ui_scale: UiScale::Auto,
            language: "auto".into(),
            ui_font_size: UiFontSize::Small,
            show_channels_in_color: false,
            dynamic_color_sliders: true,
            show_menu_colors: true,
            show_tooltips: true,
            tool_tips: ToolTips::Rich,
            show_bounding_box_when_dragging_layer: false,
            contextual_task_bar: true,
            filmstrip_shows_folder: true,
            save_and_next_saves_to: "editedFolder".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Workspace {
    pub auto_collapse_icon_panels: bool,
    pub auto_show_hidden_panels: bool,
    pub open_documents_as_tabs: bool,
    pub enable_floating_document_window_docking: bool,
    pub large_tabs: bool,
    pub enable_narrow_options_bar: bool,
    pub remember_workspace_changes: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            auto_collapse_icon_panels: false,
            auto_show_hidden_panels: true,
            open_documents_as_tabs: true,
            enable_floating_document_window_docking: true,
            large_tabs: false,
            enable_narrow_options_bar: false,
            remember_workspace_changes: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tools {
    pub show_tooltips: bool,
    /// Tool keys cycle a tool group only with ⇧ held (Photoshop's "Use Shift Key for Tool Switch").
    pub use_shift_key_for_tool_switch: bool,
    pub zoom_clicked_point_to_center: bool,
    pub enable_flick_panning: bool,
    pub vary_round_brush_hardness_on_hud: bool,
    /// Round snapped vector and transform coordinates to whole pixels.
    pub snap_vector_tools_and_transforms_to_pixel_grid: bool,
    pub show_transformation_values: bool,
    pub overscroll: bool,
    pub double_click_layer_mask_launches_select_and_mask: bool,
    /// What the right mouse button does on the canvas with the Brush and other painting tools.
    pub right_click_with_painting_tools: RightClickPaint,
    /// Pen tablets: pressure, tilt and rotation reach the brush (off: a pen paints like a mouse).
    pub use_tablet_pressure: bool,
    /// XWayland tablet compatibility on Wayland; restart to apply.
    pub tablet_compatibility_mode: String,
    /// local-image: the global pen pressure curve, `[input, output]` points in `0..=1` (Krita's
    /// tablet pressure curve), applied to stylus pressure before brush dynamics. The default
    /// line is the identity.
    pub pen_pressure_curve: Vec<[f32; 2]>,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            show_tooltips: true,
            use_shift_key_for_tool_switch: true,
            zoom_clicked_point_to_center: false,
            enable_flick_panning: true,
            vary_round_brush_hardness_on_hud: true,
            snap_vector_tools_and_transforms_to_pixel_grid: true,
            show_transformation_values: true,
            overscroll: true,
            double_click_layer_mask_launches_select_and_mask: true,
            right_click_with_painting_tools: RightClickPaint::BrushPicker,
            use_tablet_pressure: true,
            tablet_compatibility_mode: "auto".into(),
            pen_pressure_curve: photocraft_paint::pressure::LINEAR.to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HistoryLog {
    pub enabled: bool,
    pub destination: LogDestination,
    /// Text file the log is appended to (destination textFile/both).
    pub file_path: String,
    pub detail: LogDetail,
}

impl Default for HistoryLog {
    fn default() -> Self {
        Self { enabled: false, destination: LogDestination::Metadata, file_path: String::new(), detail: LogDetail::Concise }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileHandling {
    pub image_previews: Ask,
    pub lowercase_extension: bool,
    pub save_in_background: bool,
    /// Save recovery information (crash recovery autosave).
    pub autosave: bool,
    /// Minutes between autosaves of documents with unsaved changes.
    pub autosave_minutes: u32,
    /// Offer to reopen recovered documents at launch.
    pub recover_on_launch: bool,
    pub ignore_exif_profile_tag: bool,
    pub ask_before_saving_layered_tiff: bool,
    pub maximize_psd_compatibility: Ask,
    pub recent_file_count: u32,
    /// Most recently opened files, newest first (File › Open Recent).
    pub recent_files: Vec<String>,
}

impl Default for FileHandling {
    fn default() -> Self {
        Self {
            image_previews: Ask::Always,
            lowercase_extension: true,
            save_in_background: true,
            autosave: true,
            autosave_minutes: 10,
            recover_on_launch: true,
            ignore_exif_profile_tag: false,
            ask_before_saving_layered_tiff: true,
            maximize_psd_compatibility: Ask::Always,
            recent_file_count: 20,
            recent_files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Export {
    pub quick_export_format: QuickExportFormat,
    pub quick_export_location: ExportLocation,
    pub jpeg_quality: u32,
    pub metadata: ExportMetadata,
    pub convert_to_srgb: bool,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            quick_export_format: QuickExportFormat::Png,
            quick_export_location: ExportLocation::Ask,
            jpeg_quality: 85,
            metadata: ExportMetadata::Copyright,
            convert_to_srgb: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Performance {
    /// Memory PhotoCraft may use, in MB: bounds each document's pixels plus its History (the
    /// oldest states are dropped beyond it).
    pub memory_usage_mb: u32,
    /// Undo steps kept per document (History panel states).
    pub history_states: u32,
    /// Display cache levels (mip levels kept for zoomed-out views).
    pub cache_levels: u32,
    /// Side of the GPU canvas texture tiles, in pixels (applies at next launch).
    pub cache_tile_size: u32,
    /// Draw the canvas with the GPU (applies at next launch).
    pub use_gpu: bool,
    /// Explicit rendering policy. None preserves older useGpu/gpuBackend preferences.
    pub rendering_mode: Option<RenderingMode>,
    /// Graphics backend (applies at next launch; see [`GpuBackend`]).
    pub gpu_backend: GpuBackend,
    /// Memory budget of the layer-effect cache, in MB.
    pub effect_cache_mb: u32,
    pub legacy_compositing: bool,
    /// local-image: processing overrides over the values detected at start-up (applies at next launch).
    pub system: SystemResources,
}

/// local-image: Performance › Processing. Every value is an optional override; Automatic (0, `low`)
/// uses what the app derives from the hardware (`lightcraft_sysmem::profile`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SystemResources {
    /// Processing threads; 0 = Automatic.
    pub threads: u32,
    pub background_priority: BackgroundPriority,
    /// Memory heavy work may plan for, in percent of RAM; 0 = Automatic.
    pub memory_percent: u32,
}

impl Performance {
    /// Resolve old preferences without allowing legacy flags to override an explicit mode.
    pub fn effective_rendering_mode(&self) -> RenderingMode {
        self.rendering_mode.unwrap_or_else(|| if !self.use_gpu || self.gpu_backend == GpuBackend::Cpu { RenderingMode::Cpu } else { RenderingMode::Auto })
    }

    /// Pixel memory a document and its History may hold (Memory Usage), in bytes: beyond it
    /// the oldest history states are dropped.
    pub fn history_budget_bytes(&self) -> usize {
        (self.memory_usage_mb as usize).saturating_mul(1 << 20)
    }
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            memory_usage_mb: 8192,
            history_states: 50,
            cache_levels: 4,
            cache_tile_size: 8192,
            use_gpu: true,
            rendering_mode: None,
            gpu_backend: GpuBackend::Auto,
            effect_cache_mb: 768,
            legacy_compositing: false,
            system: SystemResources::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScratchDisk {
    pub path: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScratchDisks {
    /// Disks for spilling tiles once the memory budget is reached (in priority order).
    pub disks: Vec<ScratchDisk>,
}

impl Default for ScratchDisks {
    fn default() -> Self {
        Self { disks: vec![ScratchDisk { path: "(system temp)".into(), enabled: true }] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cursors {
    pub painting: PaintingCursor,
    pub show_crosshair_in_brush_tip: bool,
    pub show_only_crosshair_while_painting: bool,
    pub other: OtherCursor,
    /// Brush preview (HUD) colour.
    pub brush_preview_color: String,
}

impl Default for Cursors {
    fn default() -> Self {
        Self {
            painting: PaintingCursor::NormalTip,
            show_crosshair_in_brush_tip: false,
            show_only_crosshair_while_painting: false,
            other: OtherCursor::Standard,
            brush_preview_color: "#ff0000".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TransparencyAndGamut {
    pub grid_size: CheckerSize,
    pub grid_colors: CheckerColors,
    /// Checkerboard colours used when `gridColors` is "custom".
    pub custom_light: String,
    pub custom_dark: String,
    pub gamut_warning_color: String,
    /// Gamut warning opacity, percent.
    pub gamut_warning_opacity: u32,
}

impl Default for TransparencyAndGamut {
    fn default() -> Self {
        Self {
            grid_size: CheckerSize::Medium,
            grid_colors: CheckerColors::Light,
            custom_light: "#ffffff".into(),
            custom_dark: "#cccccc".into(),
            gamut_warning_color: "#808080".into(),
            gamut_warning_opacity: 100,
        }
    }
}

impl TransparencyAndGamut {
    /// Checker square side in screen points (`None`: no checkerboard, transparency shows white).
    pub fn square(&self) -> Option<f32> {
        match self.grid_size {
            CheckerSize::None => None,
            CheckerSize::Small => Some(4.0),
            CheckerSize::Medium => Some(8.0),
            CheckerSize::Large => Some(16.0),
        }
    }
    /// The two checkerboard colours (light, dark) as sRGB bytes.
    pub fn colors(&self) -> [[u8; 3]; 2] {
        match self.grid_colors {
            CheckerColors::Light => [[255, 255, 255], [204, 204, 204]],
            CheckerColors::Medium => [[153, 153, 153], [102, 102, 102]],
            CheckerColors::Dark => [[102, 102, 102], [51, 51, 51]],
            CheckerColors::Red => [[255, 255, 255], [255, 204, 204]],
            CheckerColors::Orange => [[255, 255, 255], [255, 229, 204]],
            CheckerColors::Green => [[255, 255, 255], [204, 255, 204]],
            CheckerColors::Blue => [[255, 255, 255], [204, 229, 255]],
            CheckerColors::Purple => [[255, 255, 255], [229, 204, 255]],
            CheckerColors::Custom => [parse_hex(&self.custom_light).unwrap_or([255; 3]), parse_hex(&self.custom_dark).unwrap_or([204; 3])],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UnitsAndRulers {
    pub rulers: Unit,
    pub type_units: TypeUnit,
    pub column_width: f64,
    pub gutter: f64,
    pub print_resolution: f64,
    pub screen_resolution: f64,
    pub point_size: PointSize,
}

impl Default for UnitsAndRulers {
    fn default() -> Self {
        Self {
            rulers: Unit::Pixels,
            type_units: TypeUnit::Points,
            column_width: 180.0,
            gutter: 12.0,
            print_resolution: 300.0,
            screen_resolution: 72.0,
            point_size: PointSize::PostScript,
        }
    }
}

impl UnitsAndRulers {
    /// Format a length in document pixels in the ruler unit, e.g. `"2.50 in"`.
    pub fn format(&self, px: f64, dpi: f64, extent: f64) -> String {
        let v = self.rulers.from_px(px, dpi, extent, self.point_size.per_inch());
        format!("{:.*}", self.rulers.decimals(), v)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GuidesGridAndSlices {
    pub guide_color: String,
    pub guide_style: LineStyle,
    pub smart_guide_color: String,
    pub grid_color: String,
    pub grid_style: LineStyle,
    /// Major gridline spacing in `gridUnit`.
    pub gridline_every: f64,
    pub grid_unit: Unit,
    pub subdivisions: u32,
    pub slice_color: String,
    pub show_slice_numbers: bool,
}

impl Default for GuidesGridAndSlices {
    fn default() -> Self {
        Self {
            guide_color: "#4affff".into(),
            guide_style: LineStyle::Lines,
            smart_guide_color: "#ff00ff".into(),
            grid_color: "#8c8c8c".into(),
            grid_style: LineStyle::Lines,
            gridline_every: 1.0,
            grid_unit: Unit::Inches,
            subdivisions: 4,
            slice_color: "#38b5ff".into(),
            show_slice_numbers: true,
        }
    }
}

impl GuidesGridAndSlices {
    /// Major gridline spacing in document pixels.
    pub fn major_px(&self, dpi: f64, extent: f64, ppi: f64) -> f64 {
        self.grid_unit.to_px(self.gridline_every.max(1e-3), dpi, extent, ppi).max(1e-3)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlugIns {
    /// Load the WebAssembly plug-ins (`*.wasm`) in `additional_plugins_folder` (native only).
    pub use_additional_plugins_folder: bool,
    pub additional_plugins_folder: String,
    pub show_extension_panels: bool,
    pub allow_scripts_to_connect: bool,
    pub generator_enabled: bool,
}

impl Default for PlugIns {
    fn default() -> Self {
        Self {
            use_additional_plugins_folder: false,
            additional_plugins_folder: String::new(),
            show_extension_panels: true,
            allow_scripts_to_connect: false,
            generator_enabled: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TypePrefs {
    pub smart_quotes: bool,
    pub missing_glyph_protection: bool,
    pub show_font_names_in_english: bool,
    pub use_esc_to_commit: bool,
    pub text_engine: TextEngine,
    pub font_preview: FontPreview,
    pub fill_new_type_layers_with_placeholder: bool,
    pub recent_fonts: u32,
}

impl Default for TypePrefs {
    fn default() -> Self {
        Self {
            smart_quotes: true,
            missing_glyph_protection: true,
            show_font_names_in_english: true,
            use_esc_to_commit: true,
            text_engine: TextEngine::WorldReady,
            font_preview: FontPreview::Medium,
            fill_new_type_layers_with_placeholder: true,
            recent_fonts: 10,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EnhancedControls {
    pub scrubby_slider_acceleration: bool,
    pub touch_gestures: bool,
    pub zoom_with_trackpad_pinch: bool,
    pub rotate_view_with_trackpad: bool,
}

impl Default for EnhancedControls {
    fn default() -> Self {
        Self { scrubby_slider_acceleration: true, touch_gestures: true, zoom_with_trackpad_pinch: true, rotate_view_with_trackpad: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RawDefaults {
    pub color_space: RawColorSpace,
    pub bit_depth: RawDepth,
    pub resolution: f64,
    pub sharpen_for: RawSharpen,
    pub open_as_smart_object: bool,
    pub apply_auto_tone: bool,
}

impl Default for RawDefaults {
    fn default() -> Self {
        Self {
            color_space: RawColorSpace::AdobeRgb,
            bit_depth: RawDepth::Sixteen,
            resolution: 300.0,
            sharpen_for: RawSharpen::None,
            open_as_smart_object: false,
            apply_auto_tone: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Integrations {
    /// Allow agents to drive the app over the loopback control channel / MCP bridge.
    pub allow_agent_control: bool,
    /// Default control port (0 = only when `--control` is given).
    pub control_port: u32,
}

impl Default for Integrations {
    fn default() -> Self {
        Self { allow_agent_control: true, control_port: 0 }
    }
}

/// Edit › Menus: hidden items and item colours, by command id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MenuCustomization {
    pub hidden: Vec<String>,
    /// Command id → colour name (red, orange, yellow, green, blue, violet, gray).
    pub colors: BTreeMap<String, String>,
}

/// Edit › Toolbar: hidden tools and a custom order (tool names as in `ui.set {tool}`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolbarCustomization {
    pub vector_keyboard: crate::vector_toolbar_layout::Settings,
    /// Before tool sets: hidden tools. Read once and turned into the set "My Tools"
    /// ([`ToolbarCustomization::migrate`]); cleared after migration.
    pub hidden: Vec<String>,
    /// Before tool sets: a custom order (see `hidden`).
    pub order: Vec<String>,
    /// Id of the tool set in use (`""` = All Tools, the default).
    pub active_set: String,
    /// The user's own tool sets (the built-ins are in [`crate::toolsets`]).
    pub sets: Vec<crate::toolsets::ToolSet>,
    /// Global Compositing persona. Old settings start in Pixel.
    #[serde(default)]
    pub persona: crate::toolsets::Persona,
    /// Vector's last set; `active_set` continues to store Pixel's original set id.
    #[serde(default)]
    pub vector_set: String,
    #[serde(default)]
    pub pixel_tool: String,
    #[serde(default)]
    pub vector_tool: String,
}

/// Immutable application bindings. Undo/redo route to the focused workspace.
pub const GLOBAL_SHORTCUTS: &[(&str, &str, &str)] = &[
    ("file.exit", "Quit", "Cmd+Q"),
    ("edit.preferences.general", "Preferences", "Cmd+K"),
    ("edit.undo", "Undo", "Cmd+Z"),
    ("edit.redo", "Redo", "Cmd+Shift+Z"),
    ("view.screenMode.cycle", "Screen mode", "F"),
    ("global.fullscreen", "Full screen", "Cmd+Shift+F"),
    ("host.library", "Library workspace", "Cmd+Alt+1"),
    ("host.develop", "Develop workspace", "Cmd+Alt+2"),
    ("host.editor", "Editor workspace", "Cmd+Alt+3"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutWorkspace {
    pub active: String,
    pub custom: BTreeMap<String, ShortcutProfile>,
}
impl Default for ShortcutWorkspace {
    fn default() -> Self {
        Self { active: "Photoshop".into(), custom: BTreeMap::new() }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShortcutProfile {
    pub parent: String,
    pub overrides: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutProfiles {
    pub library: ShortcutWorkspace,
    pub editor: ShortcutWorkspace,
    /// Distinguishes first-run migration from a deliberately selected built-in.
    pub migrated: bool,
}
impl Default for ShortcutProfiles {
    fn default() -> Self {
        Self { library: ShortcutWorkspace { active: "Lightroom".into(), ..Default::default() }, editor: ShortcutWorkspace::default(), migrated: false }
    }
}
impl ShortcutWorkspace {
    pub fn parent(&self) -> &str {
        self.custom.get(&self.active).map_or(self.active.as_str(), |p| p.parent.as_str())
    }
    pub fn duplicate(&mut self, name: &str) -> std::result::Result<(), String> {
        if name.trim().is_empty() || self.custom.contains_key(name) || ["Lightroom", "Capture One", "Photoshop", "Affinity Photo"].contains(&name) {
            return Err("Choose a unique profile name".into());
        }
        let profile =
            ShortcutProfile { parent: self.parent().into(), overrides: self.custom.get(&self.active).map(|p| p.overrides.clone()).unwrap_or_default() };
        self.custom.insert(name.into(), profile);
        self.active = name.into();
        Ok(())
    }
    pub fn rename(&mut self, name: &str) -> std::result::Result<(), String> {
        if name == self.active {
            return Ok(());
        }
        if name.trim().is_empty() || self.custom.contains_key(name) || ["Lightroom", "Capture One", "Photoshop", "Affinity Photo"].contains(&name) {
            return Err("Choose a unique profile name".into());
        }
        let profile = self.custom.remove(&self.active).ok_or("Duplicate a built-in before editing it")?;
        self.custom.insert(name.into(), profile);
        self.active = name.into();
        Ok(())
    }
    pub fn delete_active(&mut self) {
        if let Some(profile) = self.custom.remove(&self.active) {
            self.active = profile.parent;
        }
    }
}
impl ShortcutProfiles {
    pub fn workspace(&self, library: bool) -> &ShortcutWorkspace {
        if library { &self.library } else { &self.editor }
    }
    pub fn workspace_mut(&mut self, library: bool) -> &mut ShortcutWorkspace {
        if library { &mut self.library } else { &mut self.editor }
    }
    pub fn migrate(&mut self, legacy: &BTreeMap<String, String>) {
        if self.migrated || legacy.is_empty() {
            return;
        }
        for library in [false, true] {
            let overrides: BTreeMap<_, _> =
                legacy.iter().filter(|(id, _)| id.starts_with("library::") == library).map(|(k, v)| (k.clone(), v.clone())).collect();
            let workspace = self.workspace_mut(library);
            if !overrides.is_empty() && workspace.custom.is_empty() {
                workspace.custom.insert("Custom (migrated)".into(), ShortcutProfile { parent: workspace.parent().into(), overrides });
                workspace.active = "Custom (migrated)".into();
            }
        }
        self.migrated = true;
    }
    fn validate(&self, previous: &Self) -> std::result::Result<(), String> {
        for library in [false, true] {
            let workspace = self.workspace(library);
            let builtins = if library { ["Lightroom", "Capture One"] } else { ["Photoshop", "Affinity Photo"] };
            if !builtins.contains(&workspace.active.as_str()) && !workspace.custom.contains_key(&workspace.active) {
                return Err("Unknown active shortcut profile".into());
            }
            for (name, profile) in &workspace.custom {
                if name.trim().is_empty() || builtins.contains(&name.as_str()) || !builtins.contains(&profile.parent.as_str()) {
                    return Err("Invalid shortcut profile name or parent".into());
                }
                for (id, chord) in &profile.overrides {
                    if id.is_empty() || id.starts_with("library::") != library {
                        return Err("Shortcut belongs to another workspace".into());
                    }
                    let old = previous.workspace(library).custom.get(name).and_then(|p| p.overrides.get(id));
                    checked_shortcut(&Value::String(chord.clone()), old)?;
                }
            }
        }
        Ok(())
    }
}

fn get_path_mut_profile<'a>(root: &'a mut Value, path: &str) -> std::result::Result<&'a mut Value, String> {
    let (scope, rest) = path.split_once(".custom.").ok_or("Unknown profile path")?;
    let (name, field) = rest.rsplit_once('.').ok_or("Missing profile field")?;
    root.get_mut(scope).and_then(|v| v.get_mut("custom")).and_then(|v| v.get_mut(name)).and_then(|v| v.get_mut(field)).ok_or_else(|| "Unknown profile".into())
}

/// All preferences. JSON keys are the `edit.preferences.<section>` ids.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    /// Namespaced Library/Develop UI and preferences; old stores start with an empty section.
    pub library: BTreeMap<String, Value>,
    /// All AI configuration (including unknown provider/browser fields) in the one store.
    pub ai: li_ai::settings::AiSettings,
    /// Shared application palette; old preferences start with an empty book.
    #[serde(default)]
    pub user_swatches: photocraft_doc::vector_design::swatches::UserSwatches,
    pub general: General,
    pub interface: Interface,
    pub workspace: Workspace,
    pub tools: Tools,
    pub history_log: HistoryLog,
    pub file_handling: FileHandling,
    pub export: Export,
    pub performance: Performance,
    pub scratch_disks: ScratchDisks,
    pub cursors: Cursors,
    pub transparency_and_gamut: TransparencyAndGamut,
    pub units_and_rulers: UnitsAndRulers,
    pub guides_grid_and_slices: GuidesGridAndSlices,
    pub plug_ins: PlugIns,
    #[serde(rename = "type")]
    pub type_: TypePrefs,
    pub enhanced_controls: EnhancedControls,
    pub raw_defaults: RawDefaults,
    pub integrations: Integrations,
    /// Edit › Keyboard Shortcuts: command id → shortcut (`Cmd+Shift+N` notation); an empty
    /// string removes the default shortcut.
    pub shortcuts: BTreeMap<String, String>,
    pub shortcut_profiles: ShortcutProfiles,
    pub menus: MenuCustomization,
    pub toolbar: ToolbarCustomization,
    /// Edit › Check Spelling: words added to the dictionary ("Add").
    pub user_dictionary: Vec<String>,
    /// Window › Workspace › New Workspace…: saved layouts by name (JSON owned by the shell).
    pub workspaces: BTreeMap<String, Value>,
    /// Window › Workspace › Lock Workspace: panels can't be moved.
    pub workspace_locked: bool,
    /// The live panel layout (dock group heights, order, collapsed groups, open panels), saved
    /// as the user changes it and restored at launch when Workspace › Remember Workspace
    /// Changes is on. JSON owned by the shell.
    pub panel_layout: Value,
    /// Independent editor studio layouts, keyed by Pixel/Vector. Missing in old settings.
    pub persona_layouts: BTreeMap<String, Value>,
    /// The last choices of dialogs that remember them across restarts, by command id (Edit ›
    /// Fill…: `"edit.fill"` → its params). JSON owned by the shell.
    pub dialogs: BTreeMap<String, Value>,
    /// File › Scripts › Script Events Manager: event → script bindings.
    pub script_events: crate::automate_cmds::ScriptEvents,
}

/// Preferences dialog sections in Photoshop's order: (id, title).
pub const SECTIONS: [(&str, &str); 18] = [
    ("general", "General"),
    ("interface", "Interface"),
    ("workspace", "Workspace"),
    ("tools", "Tools"),
    ("historyLog", "History Log"),
    ("fileHandling", "File Handling"),
    ("export", "Export"),
    ("performance", "Performance"),
    ("scratchDisks", "Scratch Disks"),
    ("cursors", "Cursors"),
    ("transparencyAndGamut", "Transparency & Gamut"),
    ("unitsAndRulers", "Units & Rulers"),
    ("guidesGridAndSlices", "Guides, Grid & Slices"),
    ("plugIns", "Plug-ins"),
    ("type", "Type"),
    ("enhancedControls", "Enhanced Controls"),
    ("rawDefaults", "Camera Raw Defaults"),
    ("integrations", "Integrations"),
];

/// Preferences that nothing reads yet, so Edit › Preferences doesn't show them: a control that
/// does nothing is worse than a missing one (https://github.com/storytold/photocraft/issues/204).
/// They still load, save and round-trip through `prefs.get` / `prefs.set` unchanged.
///
/// When you implement one, remove it here; the `prefs_usage` test fails while a listed
/// preference is read anywhere outside `prefs.rs`, and while an unlisted one is read nowhere.
pub const HIDDEN_UNTIL_IMPLEMENTED: &[&str] = &[
    // Legacy AI compatibility key: round-tripped, but there is no automatic-start consumer/UI.
    "ai.auto_start_comfy",
    "general.colorPicker",
    "general.beepWhenDone",
    "general.exportClipboard",
    "general.resizeImageDuringPlace",
    "general.alwaysCreateSmartObjectsWhenPlacing",
    "general.animatedZoom",
    "general.zoomResizesWindows",
    "interface.showChannelsInColor",
    "interface.dynamicColorSliders",
    "workspace.autoCollapseIconPanels",
    "workspace.autoShowHiddenPanels",
    "workspace.openDocumentsAsTabs",
    "workspace.enableFloatingDocumentWindowDocking",
    "workspace.largeTabs",
    "workspace.enableNarrowOptionsBar",
    "tools.zoomClickedPointToCenter",
    "tools.enableFlickPanning",
    "tools.varyRoundBrushHardnessOnHud",
    "tools.showTransformationValues",
    "tools.doubleClickLayerMaskLaunchesSelectAndMask",
    "fileHandling.imagePreviews",
    "fileHandling.lowercaseExtension",
    "fileHandling.saveInBackground",
    "fileHandling.ignoreExifProfileTag",
    "fileHandling.maximizePsdCompatibility",
    "performance.cacheLevels",
    "performance.effectCacheMb",
    "performance.legacyCompositing",
    "scratchDisks.disks",
    "cursors.brushPreviewColor",
    "unitsAndRulers.typeUnits",
    "unitsAndRulers.columnWidth",
    "unitsAndRulers.gutter",
    "unitsAndRulers.printResolution",
    "unitsAndRulers.screenResolution",
    "plugIns.showExtensionPanels",
    "plugIns.allowScriptsToConnect",
    "plugIns.generatorEnabled",
    "type.smartQuotes",
    "type.missingGlyphProtection",
    "type.showFontNamesInEnglish",
    "type.useEscToCommit",
    "type.textEngine",
    "type.fontPreview",
    "type.fillNewTypeLayersWithPlaceholder",
    "type.recentFonts",
    "enhancedControls.scrubbySliderAcceleration",
    "enhancedControls.touchGestures",
    "enhancedControls.zoomWithTrackpadPinch",
    "enhancedControls.rotateViewWithTrackpad",
    "rawDefaults.colorSpace",
    "rawDefaults.bitDepth",
    "rawDefaults.resolution",
    "rawDefaults.sharpenFor",
    "rawDefaults.openAsSmartObject",
    "rawDefaults.applyAutoTone",
    // Agent access is governed by the launch flags (`--control`, the automation roots), not
    // by these yet.
    "integrations.allowAgentControl",
    "integrations.controlPort",
];

/// Is the preference at `path` (`"section.key"`) hidden from the Preferences dialog?
pub fn is_hidden(path: &str) -> bool {
    HIDDEN_UNTIL_IMPLEMENTED.contains(&path)
}

/// Choices of an enumerated preference (dotted path, e.g. `"cursors.painting"`).
pub fn choices(path: &str) -> Option<&'static [&'static str]> {
    Some(match path {
        "general.colorPicker" => ColorPicker::NAMES,
        "general.imageInterpolation" => Interpolation::NAMES,
        "interface.theme" => Theme::NAMES,
        "interface.toolIcons" => &["colour", "monochrome"],
        "interface.saveAndNextSavesTo" => &["editedFolder", "original", "saveAs"],
        "interface.canvasColor" => CanvasColor::NAMES,
        "interface.canvasBorder" => CanvasBorder::NAMES,
        "interface.uiScale" => UiScale::NAMES,
        "interface.uiFontSize" => UiFontSize::NAMES,
        "interface.toolTips" => ToolTips::NAMES,
        "historyLog.destination" => LogDestination::NAMES,
        "historyLog.detail" => LogDetail::NAMES,
        "fileHandling.imagePreviews" | "fileHandling.maximizePsdCompatibility" => Ask::NAMES,
        "export.quickExportFormat" => QuickExportFormat::NAMES,
        "export.quickExportLocation" => ExportLocation::NAMES,
        "export.metadata" => ExportMetadata::NAMES,
        "cursors.painting" => PaintingCursor::NAMES,
        "cursors.other" => OtherCursor::NAMES,
        "tools.rightClickWithPaintingTools" => RightClickPaint::NAMES,
        "tools.tabletCompatibilityMode" => &["auto", "on", "off"],
        "transparencyAndGamut.gridSize" => CheckerSize::NAMES,
        "transparencyAndGamut.gridColors" => CheckerColors::NAMES,
        "unitsAndRulers.rulers" | "guidesGridAndSlices.gridUnit" => Unit::NAMES,
        "unitsAndRulers.typeUnits" => TypeUnit::NAMES,
        "unitsAndRulers.pointSize" => PointSize::NAMES,
        "guidesGridAndSlices.guideStyle" | "guidesGridAndSlices.gridStyle" => LineStyle::NAMES,
        "type.textEngine" => TextEngine::NAMES,
        "type.fontPreview" => FontPreview::NAMES,
        "rawDefaults.colorSpace" => RawColorSpace::NAMES,
        "rawDefaults.bitDepth" => RawDepth::NAMES,
        "rawDefaults.sharpenFor" => RawSharpen::NAMES,
        "performance.gpuBackend" => GpuBackend::NAMES,
        "performance.renderingMode" => RenderingMode::NAMES,
        "performance.system.backgroundPriority" => BackgroundPriority::NAMES,
        _ => return None,
    })
}

/// Valid range of a numeric preference.
pub fn range(path: &str) -> Option<(f64, f64)> {
    Some(match path {
        "fileHandling.autosaveMinutes" => (1.0, 240.0),
        "fileHandling.recentFileCount" => (0.0, 100.0),
        "export.jpegQuality" => (1.0, 100.0),
        "performance.memoryUsageMb" => (256.0, 1_048_576.0),
        "performance.historyStates" => (1.0, 1000.0),
        "performance.cacheLevels" => (1.0, 8.0),
        "performance.cacheTileSize" => (256.0, 16384.0),
        "performance.effectCacheMb" => (16.0, 65536.0),
        "performance.system.threads" => (0.0, 1024.0),
        "performance.system.memoryPercent" => (0.0, 90.0),
        "transparencyAndGamut.gamutWarningOpacity" => (1.0, 100.0),
        "unitsAndRulers.columnWidth" | "unitsAndRulers.gutter" => (0.0, 10_000.0),
        "unitsAndRulers.printResolution" | "unitsAndRulers.screenResolution" | "rawDefaults.resolution" => (1.0, 10_000.0),
        "guidesGridAndSlices.gridlineEvery" => (0.001, 10_000.0),
        "guidesGridAndSlices.subdivisions" => (1.0, 100.0),
        "type.recentFonts" => (0.0, 50.0),
        "integrations.controlPort" => (0.0, 65535.0),
        _ => return None,
    })
}

/// Is this preference a `#rrggbb` colour?
pub fn is_color(path: &str) -> bool {
    if choices(path).is_some() || path.starts_with("shortcuts.") || path.starts_with("menus.") {
        return false;
    }
    let leaf = path.rsplit('.').next().unwrap_or(path);
    matches!(
        leaf,
        "guideColor"
            | "smartGuideColor"
            | "gridColor"
            | "sliceColor"
            | "canvasCustomColor"
            | "brushPreviewColor"
            | "gamutWarningColor"
            | "customLight"
            | "customDark"
    )
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let b = |i: usize| s.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    Some([b(0)?, b(2)?, b(4)?])
}

// ------------------------------------------------------------------ JSON paths

/// Map keys that are command ids (which contain dots): `shortcuts.<id>`, `menus.colors.<id>`.
fn keyed(path: &str) -> Option<(&str, &str)> {
    for prefix in ["shortcuts.", "menus.colors."] {
        if let Some(id) = path.strip_prefix(prefix) {
            return Some((&prefix[..prefix.len() - 1], id));
        }
    }
    None
}

fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(root);
    }
    if let Some((map, id)) = keyed(path) {
        return get_path(root, map)?.get(id);
    }
    path.split('.').try_fold(root, |v, k| v.get(k))
}

/// Set `value` at `path` (creating keys only inside free-form maps such as `shortcuts`).
fn set_path(root: &mut Value, path: &str, value: Value) -> std::result::Result<(), String> {
    if let Some((map, id)) = keyed(path) {
        let mut cur = &mut *root;
        for k in map.split('.') {
            cur = cur.get_mut(k).ok_or_else(|| format!("unknown preference `{path}`"))?;
        }
        cur.as_object_mut().ok_or_else(|| format!("`{map}` is not a map"))?.insert(id.to_string(), value);
        return Ok(());
    }
    let keys: Vec<&str> = path.split('.').filter(|k| !k.is_empty()).collect();
    let Some((last, parents)) = keys.split_last() else { return Err("empty path".into()) };
    let mut cur = root;
    for k in parents {
        cur = cur.get_mut(*k).ok_or_else(|| format!("unknown preference `{path}`"))?;
    }
    let free_map = matches!(parents.first(), Some(&"shortcuts")) || (parents.len() == 2 && parents[0] == "menus" && parents[1] == "colors");
    let obj = cur.as_object_mut().ok_or_else(|| format!("`{path}` is not inside a section"))?;
    if !obj.contains_key(*last) && !free_map {
        return Err(format!("unknown preference `{path}`"));
    }
    obj.insert(last.to_string(), value);
    Ok(())
}

/// Validate one value for `path` before it is stored (choices, ranges, colours).
fn check_value(path: &str, v: &Value) -> std::result::Result<(), String> {
    // PhotoCraft 8d23c622: direct edits require 2..16 unit-square point pairs.
    // Whole-section/legacy loads remain lenient; PressureCurve sanitizes their data.
    if path == "tools.penPressureCurve" {
        let valid = v.as_array().is_some_and(|points| {
            (2..=photocraft_paint::pressure::MAX_POINTS).contains(&points.len())
                && points.iter().all(|p| {
                    p.as_array().is_some_and(|p| p.len() == 2 && p.iter().all(|v| v.as_f64().is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))))
                })
        });
        if !valid {
            return Err("`tools.penPressureCurve` needs 2..16 [input, output] pairs within 0..1".into());
        }
    }
    if path == "performance.renderingMode" && v.is_null() {
        return Ok(()); // Legacy policy, resolved from useGpu and gpuBackend.
    }
    if let Some(c) = choices(path) {
        let s = v.as_str().ok_or_else(|| format!("`{path}` must be one of {}", c.join("|")))?;
        if !c.contains(&s) {
            return Err(format!("`{path}` must be one of {} (got `{s}`)", c.join("|")));
        }
    }
    if path == "interface.language" {
        let ok = v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        if !ok {
            return Err("`interface.language` must be `auto` or a language code such as `en` or `ja`".into());
        }
    }
    if let Some((lo, hi)) = range(path) {
        let x = v.as_f64().ok_or_else(|| format!("`{path}` must be a number"))?;
        if !(lo..=hi).contains(&x) {
            return Err(format!("`{path}` must be within {lo}..{hi} (got {x})"));
        }
    }
    if is_color(path) && v.as_str().and_then(parse_hex).is_none() {
        return Err(format!("`{path}` must be a #rrggbb colour"));
    }
    if let Some(sc) = path.strip_prefix("shortcuts.") {
        let s = v.as_str().ok_or_else(|| format!("shortcut for `{sc}` must be a string"))?;
        if !s.is_empty() && normalize_shortcut(s).is_none() {
            return Err(format!("`{s}` is not a valid shortcut (e.g. Cmd+Shift+N, F7)"));
        }
    }
    Ok(())
}

impl Preferences {
    /// The preferences as JSON (camelCase keys).
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Read a value by dotted path (`""` = everything).
    pub fn get(&self, path: &str) -> Option<Value> {
        let root = self.to_json();
        if let Some(rest) = path.strip_prefix("shortcutProfiles.") {
            if let Some((prefix, command)) = rest.split_once(".overrides.") {
                let mut profiles = root.get("shortcutProfiles")?.clone();
                return get_path_mut_profile(&mut profiles, &format!("{prefix}.overrides")).ok()?.get(command).cloned();
            }
            if let Some((scope, name)) = rest.split_once(".custom.") {
                let custom = root.get("shortcutProfiles")?.get(scope)?.get("custom")?;
                if let Some(profile) = custom.get(name) {
                    return Some(profile.clone());
                }
                let (profile, field) = name.rsplit_once('.')?;
                return custom.get(profile)?.get(field).cloned();
            }
        }
        get_path(&root, path).cloned()
    }

    /// Set a value by dotted path, validating it. Unknown paths and wrong types are errors.
    pub fn set(&mut self, path: &str, value: Value) -> std::result::Result<(), String> {
        let previous = self.shortcuts.clone();
        self.set_unmigrated(path, value)?;
        self.adopt_legacy_changes(&previous);
        self.toolbar.migrate();
        Ok(())
    }

    fn set_unmigrated(&mut self, path: &str, value: Value) -> std::result::Result<(), String> {
        if path == "ai" {
            self.ai = serde_json::from_value(value).map_err(|error| error.to_string())?;
            return Ok(());
        }
        if path == "library" {
            self.library = serde_json::from_value(value).map_err(|error| error.to_string())?;
            return Ok(());
        }
        if let Some(path) = path.strip_prefix("library.") {
            let mut root = serde_json::to_value(&self.library).map_err(|error| error.to_string())?;
            let mut target = &mut root;
            let fields: Vec<_> = path.split('.').collect();
            for field in &fields[..fields.len().saturating_sub(1)] {
                target = target.as_object_mut().ok_or("Library preference parent must be an object")?.entry(*field).or_insert_with(|| json!({}));
            }
            let field = fields.last().ok_or("Missing Library preference field")?;
            target.as_object_mut().ok_or("Library preference parent must be an object")?.insert((*field).into(), value);
            self.library = serde_json::from_value(root).map_err(|error| error.to_string())?;
            return Ok(());
        }

        if path == "shortcutProfiles" || path.starts_with("shortcutProfiles.") {
            let mut root = serde_json::to_value(&self.shortcut_profiles).map_err(|e| e.to_string())?;
            if path == "shortcutProfiles" {
                root = value;
            } else {
                let rest = path.trim_start_matches("shortcutProfiles.");
                if let Some((prefix, command)) = rest.split_once(".overrides.") {
                    let map = get_path_mut_profile(&mut root, &format!("{prefix}.overrides"))?.as_object_mut().ok_or("Profile overrides must be an object")?;
                    if value.is_null() {
                        map.remove(command);
                    } else {
                        map.insert(command.into(), value);
                    }
                } else if let Some((scope, name)) = rest.split_once(".custom.") {
                    let map = root.get_mut(scope).and_then(|v| v.get_mut("custom")).and_then(Value::as_object_mut).ok_or("Unknown shortcut scope")?;
                    if let Some((profile, field)) = name.rsplit_once('.').filter(|(_, field)| ["parent", "overrides"].contains(field)) {
                        let profile = map.get_mut(profile).and_then(Value::as_object_mut).ok_or("Unknown profile")?;
                        profile.insert(field.into(), value);
                    } else if value.is_null() {
                        map.remove(name);
                    } else {
                        map.insert(name.into(), value);
                    }
                } else {
                    set_path(&mut root, rest, value)?;
                }
            }
            let mut next: ShortcutProfiles = serde_json::from_value(root).map_err(|e| e.to_string())?;
            next.validate(&self.shortcut_profiles)?;
            next.migrated = true;
            for workspace in [&mut next.library, &mut next.editor] {
                for profile in workspace.custom.values_mut() {
                    for chord in profile.overrides.values_mut() {
                        if let Some(normalized) = normalize_shortcut(chord) {
                            *chord = normalized;
                        }
                    }
                }
            }
            self.shortcut_profiles = next;
            return Ok(());
        }
        if let Some(id) = path.strip_prefix("shortcuts.") {
            if id.is_empty() {
                return Err("missing shortcut command id".into());
            }
            match checked_shortcut(&value, self.shortcuts.get(id))? {
                Some(value) => {
                    self.shortcuts.insert(id.into(), value);
                }
                None => {
                    self.shortcuts.remove(id);
                }
            }
            return Ok(());
        }
        if path == "shortcuts" {
            let values = value.as_object().ok_or("shortcuts needs an object")?;
            let mut next = BTreeMap::new();
            for (id, value) in values {
                if id.is_empty() {
                    return Err("missing shortcut command id".into());
                }
                if let Some(value) = checked_shortcut(value, self.shortcuts.get(id))? {
                    next.insert(id.clone(), value);
                }
            }
            self.shortcuts = next;
            return Ok(());
        }
        check_value(path, &value)?;
        let mut root = self.to_json();
        set_path(&mut root, path, value)?;
        *self = serde_json::from_value(root).map_err(|e| format!("invalid value for `{path}`: {e}"))?;
        Ok(())
    }

    /// Reset one section (or one dotted path), or everything with `None`.
    pub fn reset(&mut self, path: Option<&str>) -> std::result::Result<(), String> {
        let Some(path) = path.filter(|p| !p.is_empty()) else {
            *self = Preferences::default();
            return Ok(());
        };
        if let Some((section, id)) = keyed(path) {
            if id.is_empty() {
                return Err(format!("missing command id in `{path}`"));
            }
            match section {
                "shortcuts" => {
                    let previous = self.shortcuts.clone();
                    self.shortcuts.remove(id);
                    self.adopt_legacy_changes(&previous);
                }
                "menus.colors" => {
                    self.menus.colors.remove(id);
                }
                _ => return Err(format!("unknown preference `{path}`")),
            }
            return Ok(());
        }
        let def = Preferences::default().get(path).ok_or_else(|| format!("unknown preference `{path}`"))?;
        if path == "shortcuts" {
            let previous = self.shortcuts.clone();
            self.shortcuts.clear();
            self.adopt_legacy_changes(&previous);
            return Ok(());
        }
        let mut root = self.to_json();
        set_path(&mut root, path, def)?;
        *self = serde_json::from_value(root).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Keep older automation and workspace-layout writes useful after migration.
    /// The legacy map remains serializable; only workspaces changed by that write move.
    fn adopt_legacy_changes(&mut self, previous: &BTreeMap<String, String>) {
        if !self.shortcut_profiles.migrated {
            self.shortcut_profiles.migrate(&self.shortcuts);
            return;
        }
        for library in [false, true] {
            let scoped = |map: &BTreeMap<String, String>| -> BTreeMap<String, String> {
                map.iter().filter(|(id, _)| id.starts_with("library::") == library).map(|(id, key)| (id.clone(), key.clone())).collect()
            };
            let overrides = scoped(&self.shortcuts);
            if overrides == scoped(previous) {
                continue;
            }
            let workspace = self.shortcut_profiles.workspace_mut(library);
            if !workspace.custom.contains_key(&workspace.active) && !overrides.is_empty() {
                let mut name = format!("Custom ({})", workspace.active);
                while workspace.custom.contains_key(&name) {
                    name.push('2');
                }
                let _ = workspace.duplicate(&name);
            }
            if let Some(profile) = workspace.custom.get_mut(&workspace.active) {
                for id in scoped(previous).keys() {
                    if !overrides.contains_key(id) {
                        profile.overrides.remove(id);
                    }
                }
                for (id, key) in overrides {
                    if previous.get(&id) != Some(&key) {
                        profile.overrides.insert(id, key);
                    }
                }
            }
        }
    }

    /// Active custom bindings, with the legacy map readable until migration.
    pub fn shortcut_overrides(&self, library: bool) -> &BTreeMap<String, String> {
        let workspace = self.shortcut_profiles.workspace(library);
        if let Some(profile) = workspace.custom.get(&workspace.active) {
            &profile.overrides
        } else if !self.shortcut_profiles.migrated {
            &self.shortcuts
        } else {
            static EMPTY: std::sync::LazyLock<BTreeMap<String, String>> = std::sync::LazyLock::new(BTreeMap::new);
            &EMPTY
        }
    }

    /// The shortcut in effect for a command: the user's override, else its default.
    pub fn shortcut<'a>(&'a self, id: &str, default: Option<&'a str>) -> Option<&'a str> {
        if let Some((_, _, chord)) = GLOBAL_SHORTCUTS.iter().find(|(command, _, _)| *command == id) {
            return Some(chord);
        }
        match self.shortcut_overrides(id.starts_with("library::")).get(id) {
            Some(s) if s.is_empty() => None,
            Some(s) => (!global_shortcut_conflict(s)).then_some(s.as_str()),
            None => default,
        }
    }
}

// ------------------------------------------------------------------ shortcuts

/// Fixed Global chords also protect spring-loaded tools resolved by the engine.
pub fn global_shortcut_conflict(chord: &str) -> bool {
    global_shortcut_owner(chord).is_some()
}
fn global_shortcut_owner(chord: &str) -> Option<&'static str> {
    let canonical =
        |s: &str| normalize_shortcut(s).map(|s| if cfg!(target_os = "macos") { s } else { s.replace("Cmd+Ctrl+", "Cmd+").replace("Ctrl+", "Cmd+") });
    let key = canonical(chord)?;
    GLOBAL_SHORTCUTS.iter().find(|(_, _, chord)| canonical(chord).as_ref() == Some(&key)).map(|(id, _, _)| *id)
}

// Unchanged legacy spellings may survive a full dialog round trip; newly introduced
// invalid keys are refused. Null means remove the override, empty means disable it.
fn checked_shortcut(value: &Value, previous: Option<&String>) -> std::result::Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) if s.is_empty() => Ok(Some(String::new())),
        Value::String(s) => {
            normalize_shortcut(s).or_else(|| previous.filter(|old| *old == s).cloned()).map(Some).ok_or_else(|| format!("`{s}` is not a valid shortcut"))
        }
        _ => Err("shortcut must be a string or null".into()),
    }
}

/// Canonical spelling of a shortcut: modifiers in `Cmd+Ctrl+Alt+Shift` order, key last
/// (single characters upper-cased). `None` if it has no key or an unknown modifier.
pub fn normalize_shortcut(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (mut cmd, mut ctrl, mut alt, mut shift) = (false, false, false, false);
    let mut key: Option<String> = None;
    // A trailing "+" key is spelled "Cmd++".
    let (body, plus_key) = if s.ends_with("++") || s == "+" { (s.trim_end_matches('+'), true) } else { (s, false) };
    for part in body.split('+').filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "⌘" | "meta" | "super" => cmd = true,
            "ctrl" | "control" | "⌃" => ctrl = true,
            "alt" | "option" | "opt" | "⌥" => alt = true,
            "shift" | "⇧" => shift = true,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = Some(shortcut_key(part)?);
            }
        }
    }
    if plus_key {
        if key.is_some() {
            return None;
        }
        key = Some("+".into());
    }
    let key = key?;
    let mut out = Vec::new();
    if cmd {
        out.push("Cmd");
    }
    if ctrl {
        out.push("Ctrl");
    }
    if alt {
        out.push("Alt");
    }
    if shift {
        out.push("Shift");
    }
    let mut s = out.join("+");
    if !s.is_empty() {
        s.push('+');
    }
    s.push_str(&key);
    Some(s)
}

/// Platform-independent key vocabulary. UI regression tests check every egui key against
/// this table, so persisted spellings stay executable without making the engine depend on egui.
fn shortcut_key(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "arrowdown" | "down" | "⏷" => "Down",
        "arrowleft" | "left" | "⏴" => "Left",
        "arrowright" | "right" | "⏵" => "Right",
        "arrowup" | "up" | "⏶" => "Up",
        "esc" | "escape" => "Escape",
        "return" | "numpadenter" | "enter" => "Enter",
        "help" | "insert" => "Insert",
        "numpadcomma" | "comma" | "," => ",",
        "numpadsubtract" | "minus" | "−" | "-" => "-",
        "numpaddecimal" | "period" | "." => ".",
        "numpadadd" | "plus" | "+" => "+",
        "numpadequal" | "equal" | "equals" | "=" => "=",
        "colon" | ":" => ":",
        "semicolon" | ";" => ";",
        "backslash" | "\\" => "\\",
        "numpaddivide" | "slash" | "/" => "/",
        "pipe" | "|" => "|",
        "questionmark" | "?" => "?",
        "exclamationmark" | "!" => "!",
        "bracketleft" | "openbracket" | "[" => "[",
        "bracketright" | "closebracket" | "]" => "]",
        "opencurlybracket" | "{" => "OpenCurlyBracket",
        "closecurlybracket" | "}" => "CloseCurlyBracket",
        "backtick" | "backquote" | "grave" | "`" => "Backtick",
        "quote" | "'" => "'",
        "metaleft" | "osleft" => "SuperLeft",
        "metaright" | "osright" => "SuperRight",
        _ => {
            const NAMES: &[&str] = &[
                "Tab",
                "Backspace",
                "Delete",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "Copy",
                "Cut",
                "Paste",
                "Space",
                "BrowserBack",
                "ShiftLeft",
                "ShiftRight",
                "ControlLeft",
                "ControlRight",
                "AltLeft",
                "AltRight",
                "SuperLeft",
                "SuperRight",
                "IntlBackslash",
            ];
            if let Some(name) = NAMES.iter().find(|name| name.eq_ignore_ascii_case(s)) {
                return Some((*name).into());
            }
            let digit = lower.strip_prefix("digit").or_else(|| lower.strip_prefix("numpad")).unwrap_or(&lower);
            if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
                return Some(digit.into());
            }
            let letter = lower.strip_prefix("key").unwrap_or(&lower);
            if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() {
                return Some(letter.to_ascii_uppercase());
            }
            if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=35).contains(n)) {
                return Some(format!("F{number}"));
            }
            return None;
        }
    };
    Some(canonical.into())
}

/// Hold-and-release gestures (#249): temporary tools that last while their key is held and give
/// the previous tool back on release. They are bound like shortcuts (Edit › Keyboard Shortcuts ›
/// Tools › Temporary, `edit.keyboardShortcuts`) but held rather than pressed, so they are not
/// commands; the UI reads their keys through [`Preferences::shortcut`]. While a selection or
/// shape is being dragged, the Hand key repositions it instead of panning (Photoshop's Space).
pub const TEMPORARY_TOOLS: &[(&str, &str, &str)] = &[
    ("tools.temporary.hand", "Hand Tool (hold)", "Space"),
    ("tools.temporary.zoomIn", "Zoom In (hold, drag to scrub)", "Cmd+Space"),
    ("tools.temporary.zoomOut", "Zoom Out (hold)", "Cmd+Alt+Space"),
];

/// Bindable toolbar groups. A group shares one shortcut, including its flyout tools.
pub const TOOL_SHORTCUTS: &[(&str, &str, Option<&str>)] = &[
    ("tools.select.Move", "Move Tool", Some("V")),
    ("tools.select.RectMarquee", "Marquee Tools", Some("M")),
    ("tools.select.Lasso", "Lasso Tools", Some("L")),
    ("tools.select.MagicWand", "Selection Tools", Some("W")),
    ("tools.select.Crop", "Crop and Slice Tools", Some("C")),
    ("tools.select.Eyedropper", "Eyedropper and Measurement Tools", Some("I")),
    ("tools.select.Brush", "Brush Tools", Some("B")),
    ("tools.select.Eraser", "Eraser Tools", Some("E")),
    ("tools.select.Gradient", "Fill Tools", Some("G")),
    ("tools.select.Type", "Type Tools", Some("T")),
    ("tools.select.Hand", "Hand Tool", Some("H")),
    ("tools.select.RotateView", "Rotate View Tool", Some("R")),
    ("tools.select.Zoom", "Zoom Tool", Some("Z")),
    ("tools.select.SpotHealing", "Healing and Remove Tools", Some("J")),
    ("tools.select.CloneStamp", "Clone Stamp Tool", Some("S")),
    ("tools.select.HistoryBrush", "History Brush Tool", Some("Y")),
    ("tools.select.Blur", "Blur, Sharpen and Smudge Tools", None),
    ("tools.select.Dodge", "Dodge, Burn and Sponge Tools", Some("O")),
    ("tools.select.Pen", "Pen Tool", Some("P")),
    ("tools.select.PathSelection", "Path Selection Tools", Some("A")),
    ("tools.select.Rectangle", "Shape Tools", Some("U")),
    ("tools.select.AiRemove", "AI Remove Tool", None),
    ("tools.select.AiCutout", "AI Cutout Tool", Some("K")),
];

/// Every bindable id with its default: commands, then the temporary tools.
fn bindable() -> impl Iterator<Item = (&'static str, Option<&'static str>)> {
    let mut ids = BTreeMap::new();
    for (id, shortcut) in crate::command_specs()
        .iter()
        .map(|c| (c.id, c.shortcut))
        .chain(TEMPORARY_TOOLS.iter().map(|t| (t.0, Some(t.2))))
        .chain(TOOL_SHORTCUTS.iter().map(|t| (t.0, t.2)))
    {
        let current = ids.entry(id).or_insert(None);
        if current.is_none() {
            *current = shortcut;
        }
    }
    ids.into_iter()
}

/// Shortcuts bound to more than one command: (shortcut, ids). `bindings` are (id, shortcut).
pub fn conflicts<'a>(bindings: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<(String, Vec<String>)> {
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, sc) in bindings {
        if let Some(n) = normalize_shortcut(sc) {
            let ids = by.entry(n).or_default();
            if !ids.iter().any(|i| i == id) {
                ids.push(id.to_string());
            }
        }
    }
    by.into_iter().filter(|(_, ids)| ids.len() > 1).collect()
}

// ------------------------------------------------------------------ store

/// The session's preferences plus a revision counter that moves on every change (frontends
/// persist when it moves).
#[derive(Clone, Debug, Default)]
pub struct PrefsStore {
    values: Preferences,
    rev: u64,
}

impl PrefsStore {
    pub fn get(&self) -> &Preferences {
        &self.values
    }
    /// Change preferences; bumps the revision.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Preferences) -> R) -> R {
        self.rev += 1;
        f(&mut self.values)
    }
    pub fn rev(&self) -> u64 {
        self.rev
    }
}

impl Session {
    /// The current preferences.
    pub fn prefs(&self) -> &Preferences {
        self.prefs.get()
    }

    /// Change preferences and apply the engine-side ones.
    pub fn edit_prefs<R>(&mut self, f: impl FnOnce(&mut Preferences) -> R) -> R {
        let r = self.prefs.edit(f);
        self.apply_prefs();
        r
    }

    /// Push engine-relevant preferences into live state: every document's history limit and
    /// the layer-effect cache budget.
    pub fn apply_prefs(&mut self) {
        let n = self.prefs().performance.history_states.max(1) as usize;
        let bytes = self.prefs().performance.history_budget_bytes();
        let budget = self.prefs().performance.effect_cache_mb as usize;
        for st in &mut self.docs {
            st.history.max_states = n;
            st.history.max_bytes = bytes;
            st.history.trim(&st.doc);
        }
        photocraft_compose::set_effect_cache_budget(budget << 20);
        crate::plugin_cmds::sync_prefs(self);
    }

    /// Everything persisted as one JSON document: the preferences plus `colorSettings`.
    pub fn prefs_to_json(&self) -> String {
        serde_json::to_string_pretty(&self.prefs_value()).unwrap_or_default()
    }

    /// [`Session::prefs_to_json`] as a JSON tree (frontends merge it with what storage holds).
    pub fn prefs_value(&self) -> Value {
        let mut v = self.prefs().to_json();
        if let Value::Object(m) = &mut v {
            m.insert("colorSettings".into(), serde_json::to_value(&self.color.settings).unwrap_or(Value::Null));
            m.insert("version".into(), json!(1));
            m.insert("presets".into(), self.presets.to_json(self));
        }
        v
    }

    /// Restore preferences saved by [`Session::prefs_to_json`]. Missing keys keep their
    /// defaults and unknown keys are ignored, so files from older and newer versions load.
    pub fn load_prefs_json(&mut self, s: &str) -> std::result::Result<(), String> {
        let mut v: Value = serde_json::from_str(s).map_err(|e| format!("preferences: {e}"))?;
        let color = v.as_object_mut().and_then(|m| m.remove("colorSettings"));
        let presets = v.as_object_mut().and_then(|m| m.remove("presets"));
        let (mut prefs, mut rejected) = lenient::<Preferences>(v).map_err(|e| format!("preferences: {e}"))?;
        // Repair old mixed-case named keys while retaining disabled or unrecognised user
        // entries. Validation of new assignments is strict; loading never discards a binding.
        for shortcut in prefs.shortcuts.values_mut() {
            if let Some(normalized) = normalize_shortcut(shortcut) {
                *shortcut = normalized;
            }
        }
        prefs.shortcut_profiles.migrate(&prefs.shortcuts);
        for library in [false, true] {
            let builtins = if library { ["Lightroom", "Capture One"] } else { ["Photoshop", "Affinity Photo"] };
            let workspace = prefs.shortcut_profiles.workspace_mut(library);
            for profile in workspace.custom.values_mut() {
                if !builtins.contains(&profile.parent.as_str()) {
                    profile.parent = builtins[0].into();
                }
                for chord in profile.overrides.values_mut() {
                    if let Some(normalized) = normalize_shortcut(chord) {
                        *chord = normalized;
                    }
                }
            }
            if !builtins.contains(&workspace.active.as_str()) && !workspace.custom.contains_key(&workspace.active) {
                workspace.active = builtins[0].into();
            }
        }
        if let Some(c) = color {
            let (settings, bad) = lenient(c).map_err(|e| format!("colour settings: {e}"))?;
            self.color.settings = settings;
            rejected.extend(bad.into_iter().map(|p| format!("colorSettings.{p}")));
            photocraft_compose::psblend::set_text_gamma(self.color.settings.blend_text_gamma);
        }
        self.prefs.edit(|p| {
            *p = prefs;
            p.toolbar.migrate();
        });
        if let Some(v) = presets {
            self.load_presets_json(v);
        }
        self.apply_prefs();
        if rejected.is_empty() { Ok(()) } else { Err(format!("invalid fields used defaults: {}", rejected.join(", "))) }
    }

    /// Full JSON view used by `prefs.get`: preferences plus `colorSettings`.
    fn prefs_view(&self) -> Value {
        let mut v = self.prefs().to_json();
        if let Value::Object(m) = &mut v {
            m.insert("colorSettings".into(), serde_json::to_value(&self.color.settings).unwrap_or(Value::Null));
        }
        v
    }

    /// Set one dotted path, routing `colorSettings.*` to the colour state.
    fn set_pref(&mut self, path: &str, value: Value) -> std::result::Result<(), String> {
        if let Some(rest) = path.strip_prefix("colorSettings") {
            let mut cur = serde_json::to_value(&self.color.settings).map_err(|e| e.to_string())?;
            let rest = rest.trim_start_matches('.');
            if rest.is_empty() {
                cur = value;
            } else {
                set_path(&mut cur, rest, value)?;
            }
            self.color.settings = serde_json::from_value(cur).map_err(|e| format!("invalid value for `{path}`: {e}"))?;
            crate::color_cmds::validate_settings(&self.color.settings)?;
            self.prefs.edit(|_| ());
            return Ok(());
        }
        let mut next = self.prefs().clone();
        if path.is_empty() || SECTIONS.iter().any(|(id, _)| *id == path) || matches!(path, "shortcuts" | "menus" | "toolbar" | "library" | "ai") {
            // Whole section (or everything): merge the object key by key so each value is checked.
            let obj = value.as_object().ok_or_else(|| format!("`{path}` needs an object"))?;
            for (k, v) in obj {
                let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                if path.is_empty() && k == "colorSettings" {
                    self.set_pref("colorSettings", v.clone())?;
                    continue;
                }
                if v.is_object()
                    && p != "shortcutProfiles"
                    && !p.starts_with("shortcuts")
                    && !p.ends_with(".colors")
                    && next.get(&p).is_some_and(|c| c.is_object())
                {
                    for (k2, v2) in v.as_object().into_iter().flatten() {
                        next.set_unmigrated(&format!("{p}.{k2}"), v2.clone())?;
                    }
                } else {
                    next.set_unmigrated(&p, v.clone())?;
                }
            }
        } else {
            next.set_unmigrated(path, value)?;
        }
        next.adopt_legacy_changes(&self.prefs().shortcuts);
        self.prefs.edit(|p| *p = next);
        Ok(())
    }
}

// ------------------------------------------------------------------ commands

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn prefs_get(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p.get("path").and_then(Value::as_str).unwrap_or("");
    let v = s.prefs_view();
    s.prefs().get(path).or_else(|| get_path(&v, path).cloned()).ok_or_else(|| bad("prefs.get", format!("unknown preference `{path}`")))
}

fn prefs_set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "prefs.set";
    let mut changes: Vec<(String, Value)> = Vec::new();
    if let Some(path) = p.get("path").and_then(Value::as_str) {
        changes.push((path.to_string(), p.get("value").cloned().ok_or_else(|| bad(cmd, "missing `value`"))?));
    }
    if let Some(m) = p.get("values").and_then(Value::as_object) {
        changes.extend(m.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    if changes.is_empty() {
        return Err(bad(cmd, "give `path` and `value`, or `values: {path: value, …}`"));
    }
    // All or nothing: validate against a scratch copy first.
    let (saved_prefs, saved_color) = (s.prefs().clone(), s.color.settings.clone());
    for (path, v) in &changes {
        if let Err(e) = s.set_pref(path, v.clone()) {
            s.prefs.edit(|p| *p = saved_prefs);
            s.color.settings = saved_color;
            return Err(bad(cmd, e));
        }
    }
    s.prefs.edit(|p| p.toolbar.migrate());
    s.apply_prefs();
    let view = s.prefs_view();
    let out: Map<String, Value> = changes.iter().map(|(k, _)| (k.clone(), get_path(&view, k).cloned().unwrap_or(Value::Null))).collect();
    Ok(Value::Object(out))
}

fn prefs_reset(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p.get("path").or_else(|| p.get("section")).and_then(Value::as_str);
    match path {
        Some("colorSettings") => s.color.settings = Default::default(),
        // One colour setting: `Preferences` has no `colorSettings`, so take the default
        // from `ColorSettings` and route it like `prefs.set` does.
        Some(path) if path.starts_with("colorSettings.") => {
            let defaults = serde_json::to_value(crate::color_cmds::ColorSettings::default()).map_err(|e| bad("prefs.reset", e.to_string()))?;
            let key = path.strip_prefix("colorSettings.").unwrap_or(path);
            let def = get_path(&defaults, key).cloned().ok_or_else(|| bad("prefs.reset", format!("unknown preference `{path}`")))?;
            s.set_pref(path, def).map_err(|e| bad("prefs.reset", e))?;
        }
        None => {
            s.color.settings = Default::default();
            s.edit_prefs(|p| p.reset(None)).map_err(|e| bad("prefs.reset", e))?;
        }
        Some(path) => {
            let mut next = s.prefs().clone();
            next.reset(Some(path)).map_err(|e| bad("prefs.reset", e))?;
            next.adopt_legacy_changes(&s.prefs().shortcuts);
            s.edit_prefs(|p| *p = next);
        }
    }
    s.prefs.edit(|_| ());
    s.apply_prefs();
    // A keyed reset removes an override; absence is the successful result, not
    // an unknown-path error from querying the deleted map entry.
    if path.and_then(keyed).is_some() { Ok(Value::Null) } else { prefs_get(s, &json!({"path": path.unwrap_or("")})) }
}

/// `edit.preferences.<section>`: the section's values (the GUI opens the dialog on it instead).
fn preferences_section(s: &mut Session, p: &Value) -> Result<Value> {
    let section = p.get("__section").and_then(Value::as_str).unwrap_or("general").to_string();
    let title = SECTIONS.iter().find(|(id, _)| *id == section).map_or("General", |(_, t)| *t);
    let values = s.prefs().get(&section).unwrap_or(Value::Null);
    Ok(json!({"section": section, "title": title, "values": values}))
}

/// Edit › Keyboard Shortcuts (engine part): list, change and reset application shortcuts.
fn keyboard_shortcuts(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "edit.keyboardShortcuts";
    if p.get("set").is_some_and(|value| !value.is_object()) {
        return Err(bad(cmd, "set needs an object"));
    }
    let before = s.prefs().clone();
    let mut next = before.clone();
    let mut changed = false;
    if let Some(r) = p.get("reset") {
        match r {
            Value::Bool(true) => {
                next.shortcuts.clear();
                changed = true;
            }
            Value::Array(ids) => {
                changed = true;
                for id in ids.iter().filter_map(Value::as_str) {
                    next.shortcuts.remove(id);
                }
            }
            _ => {}
        }
    }
    if let Some(m) = p.get("set").and_then(Value::as_object) {
        changed = true;
        for (id, v) in m {
            if !bindable().any(|(b, _)| b == id) && !p.get("allowUnknown").and_then(Value::as_bool).unwrap_or(false) {
                return Err(bad(cmd, format!("unknown command `{id}`")));
            }
            match checked_shortcut(v, before.shortcuts.get(id)).map_err(|e| bad(cmd, e))? {
                Some(value) => {
                    next.shortcuts.insert(id.clone(), value);
                }
                None => {
                    next.shortcuts.remove(id);
                }
            }
        }
        // Like Photoshop, a shortcut moved to a new command is taken away from the old one.
        if p.get("removeConflicts").and_then(Value::as_bool).unwrap_or(true) {
            for (id, v) in m {
                let Some(sc) = v.as_str().and_then(normalize_shortcut) else { continue };
                if global_shortcut_conflict(&sc) {
                    continue;
                }
                for (c, def) in bindable() {
                    // Never strip a command that this same call is assigning:
                    // two entries for one key are a clash inside the call, which
                    // the returned conflicts list reports (#719). Stripping
                    // each other here unbound both silently.
                    if c == id || m.contains_key(c) || GLOBAL_SHORTCUTS.iter().any(|(global, _, _)| *global == c) {
                        continue;
                    }
                    if next.shortcut(c, def).and_then(normalize_shortcut).as_deref() == Some(sc.as_str()) {
                        next.shortcuts.insert(c.to_string(), String::new());
                    }
                }
            }
        }
    }
    if changed {
        next.adopt_legacy_changes(&before.shortcuts);
        s.edit_prefs(|p| *p = next);
    }
    let prefs = s.prefs();
    let filter = p.get("filter").and_then(Value::as_str).map(str::to_ascii_lowercase);
    let temporary = TEMPORARY_TOOLS.iter().map(|t| (t.0, t.1, &["Tools", "Temporary"][..], Some(t.2), true));
    let list: Vec<Value> = crate::command_specs()
        .iter()
        .map(|c| (c.id, c.label, c.menu, c.shortcut, false))
        .chain(temporary)
        .chain(TOOL_SHORTCUTS.iter().map(|t| (t.0, t.1, &["Tools"][..], t.2, false)))
        .filter(|c| filter.as_ref().is_none_or(|f| c.0.to_ascii_lowercase().contains(f) || c.1.to_ascii_lowercase().contains(f)))
        .filter(|c| p.get("list").and_then(Value::as_bool).unwrap_or(false) || filter.is_some() || prefs.shortcuts.contains_key(c.0))
        .map(|(id, label, menu, def, hold)| {
            let mut v = json!({"id": id, "label": label, "menu": menu, "default": def, "shortcut": prefs.shortcut(id, def)});
            if hold {
                v["hold"] = json!(true);
            }
            v
        })
        .collect();
    let bindings: Vec<(&str, &str)> = bindable().filter_map(|(id, def)| Some((id, prefs.shortcut(id, def)?))).collect();
    let mut conflicts: Vec<Value> = conflicts(bindings).into_iter().map(|(sc, ids)| json!({"shortcut": sc, "commands": ids})).collect();
    for library in [false, true] {
        for (id, key) in prefs.shortcut_overrides(library).iter().filter(|(id, _)| id.starts_with("library::") == library) {
            if let Some(global) = global_shortcut_owner(key)
                && id != global
            {
                conflicts
                    .push(json!({"shortcut": key, "commands":[id,global], "scope":if library { "library" } else { "editor" }, "global":true,"ignored":true}));
            }
        }
    }
    Ok(json!({"overrides": prefs.shortcuts, "commands": list, "conflicts": conflicts}))
}

/// Edit › Menus: hide/show items and give them colours.
fn menus(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = |k: &str| -> Vec<String> {
        p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
    };
    const COLORS: [&str; 8] = ["none", "red", "orange", "yellow", "green", "blue", "violet", "gray"];
    if let Some(m) = p.get("color").and_then(Value::as_object) {
        for v in m.values() {
            if let Some(c) = v.as_str().filter(|c| !COLORS.contains(c)) {
                return Err(bad("edit.menus", format!("unknown colour `{c}` ({})", COLORS.join("|"))));
            }
        }
    }
    let (hide, show) = (ids("hide"), ids("show"));
    s.edit_prefs(|pr| {
        if p.get("reset").and_then(Value::as_bool) == Some(true) {
            pr.menus = MenuCustomization::default();
        }
        for id in hide {
            if !pr.menus.hidden.contains(&id) {
                pr.menus.hidden.push(id);
            }
        }
        pr.menus.hidden.retain(|h| !show.contains(h));
        if let Some(m) = p.get("color").and_then(Value::as_object) {
            for (id, v) in m {
                match v.as_str() {
                    Some(c) if c != "none" => {
                        pr.menus.colors.insert(id.clone(), c.to_string());
                    }
                    _ => {
                        pr.menus.colors.remove(id);
                    }
                }
            }
        }
    });
    Ok(serde_json::to_value(&s.prefs().menus).unwrap_or_default())
}

/// Edit › Toolbar: hide tools and reorder the toolbar. The change lands in the active custom
/// tool set; with a built-in set (read-only) active it first becomes a copy, "My Tools".
fn toolbar(s: &mut Session, p: &Value) -> Result<Value> {
    use crate::toolsets::{Persona, ToolSet};
    let ids = |k: &str| -> Option<Vec<String>> { p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()) };
    s.edit_prefs(|pr| {
        if p.get("reset").and_then(Value::as_bool) == Some(true) {
            match pr.toolbar.persona {
                Persona::Pixel => pr.toolbar.active_set = crate::toolsets::DEFAULT_ID.into(),
                Persona::Vector => pr.toolbar.vector_set = pr.toolbar.persona.default_set().into(),
            }
        }
        let (hidden, order) = (ids("hidden"), ids("order"));
        if hidden.is_none() && order.is_none() {
            return;
        }
        let active = pr.toolbar.active();
        if ToolbarCustomization::is_builtin(&active.id) {
            let id = (1u32..).map(|n| format!("custom-{n}")).find(|id| !pr.toolbar.sets.iter().any(|x| &x.id == id)).unwrap_or_default();
            let name = pr.toolbar.unique_name("My Tools");
            pr.toolbar.sets.push(ToolSet { id: id.clone(), name, persona: pr.toolbar.persona, tools: active.tools.clone() });
            match pr.toolbar.persona {
                Persona::Pixel => pr.toolbar.active_set = id,
                Persona::Vector => pr.toolbar.vector_set = id,
            }
        }
        let available = crate::toolsets::tools_for(pr.toolbar.persona);
        let canon = |t: &str| crate::toolsets::canonical_tool(t).filter(|id| available.contains(id)).map(str::to_owned);
        let id = match pr.toolbar.persona {
            Persona::Pixel => pr.toolbar.active_set.clone(),
            Persona::Vector => pr.toolbar.vector_set.clone(),
        };
        if let Some(set) = pr.toolbar.sets.iter_mut().find(|x| x.id == id) {
            let mut tools: Vec<String> = Vec::new();
            for t in order.iter().flatten().filter_map(|t| canon(t)).chain(set.tools.iter().cloned()).chain(available.iter().map(|t| t.to_string())) {
                if !tools.contains(&t) {
                    tools.push(t);
                }
            }
            let keep: Vec<String> = match &hidden {
                Some(h) => {
                    let h: Vec<String> = h.iter().filter_map(|t| canon(t)).collect();
                    tools.retain(|t| !h.contains(t));
                    tools
                }
                None => {
                    tools.retain(|t| set.tools.contains(t));
                    tools
                }
            };
            set.tools = keep;
        }
    });
    let mut out = serde_json::to_value(&s.prefs().toolbar).unwrap_or_default();
    out["active"] = json!(s.prefs().toolbar.active().id);
    Ok(out)
}

macro_rules! spec {
    ($id:literal, $label:literal, [$($m:literal),*], $sc:expr, $params:literal, $run:expr, $journal:expr) => {
        CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: $sc, params: $params, enabled: always, run: $run, journal: $journal }
    };
}

macro_rules! section {
    ($id:literal, $label:literal) => {
        CommandSpec { id: $id, label: $label, menu: &["Edit", "Preferences"], shortcut: None, params: r##"{}"##, enabled: always, run: |s, _| preferences_section(s, &json!({"__section": section_of($id)})), journal: false }
    };
}

fn section_of(id: &str) -> &str {
    id.rsplit('.').next().unwrap_or("general")
}

/// Preference command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "prefs.get",
            "Get Preferences",
            [],
            None,
            r##"{"path":"section.key"?=everything (e.g. "performance.historyStates", "colorSettings.workingRgb")}"##,
            prefs_get,
            false
        ),
        spec!(
            "prefs.set",
            "Set Preferences",
            [],
            None,
            r##"{"path":"section.key","value":json} or {"values":{"section.key":json,…}} (validated; all or nothing)"##,
            prefs_set,
            false
        ),
        spec!("prefs.reset", "Reset Preferences", [], None, r##"{"path":"section|section.key"?=everything}"##, prefs_reset, false),
        section!("edit.preferences.general", "General…"),
        section!("edit.preferences.interface", "Interface…"),
        section!("edit.preferences.workspace", "Workspace…"),
        section!("edit.preferences.tools", "Tools…"),
        section!("edit.preferences.historyLog", "History Log…"),
        section!("edit.preferences.fileHandling", "File Handling…"),
        section!("edit.preferences.export", "Export…"),
        section!("edit.preferences.performance", "Performance…"),
        section!("edit.preferences.scratchDisks", "Scratch Disks…"),
        section!("edit.preferences.cursors", "Cursors…"),
        section!("edit.preferences.transparencyAndGamut", "Transparency & Gamut…"),
        section!("edit.preferences.unitsAndRulers", "Units & Rulers…"),
        section!("edit.preferences.guidesGridAndSlices", "Guides, Grid & Slices…"),
        section!("edit.preferences.plugIns", "Plug-ins…"),
        section!("edit.preferences.type", "Type…"),
        section!("edit.preferences.enhancedControls", "Enhanced Controls…"),
        section!("edit.preferences.rawDefaults", "Camera Raw…"),
        section!("edit.preferences.integrations", "Integrations…"),
        spec!(
            "edit.keyboardShortcuts",
            "Keyboard Shortcuts…",
            ["Edit"],
            Some("Cmd+Alt+Shift+K"),
            r##"{"set":{"<command id>|tools.temporary.hand|zoomIn|zoomOut":"Cmd+Shift+X"|""(remove)|null(default)}?,"reset":true|["<id>",…]?,"removeConflicts":bool=true,"filter":str?,"list":bool=false}"##,
            keyboard_shortcuts,
            true
        ),
        spec!(
            "edit.menus",
            "Menus…",
            ["Edit"],
            Some("Cmd+Alt+Shift+M"),
            r##"{"hide":["<id>",…]?,"show":["<id>",…]?,"color":{"<id>":"red|orange|yellow|green|blue|violet|gray|none"}?,"reset":bool=false}"##,
            menus,
            true
        ),
        spec!("edit.toolbar", "Toolbar…", ["Edit"], None, r##"{"hidden":["<tool>",…]?,"order":["<tool>",…]?,"reset":bool=false}"##, toolbar, true),
    ]
}

#[cfg(test)]
#[path = "prefs/tests.rs"]
mod tests;

#[cfg(test)]
mod shortcut_profile_tests {
    use super::*;

    #[test]
    fn keys_profiles_migrate_legacy_and_downgrade_read() {
        let file = r#"{"shortcuts":{"library::view.develop":"F6","tools.select.Brush":"Shift+B","future.command":"Alt+FutureKey"}}"#;
        let mut session = Session::new();
        session.load_prefs_json(file).unwrap();
        let prefs = session.prefs();
        assert_eq!(prefs.shortcut_profiles.library.active, "Custom (migrated)");
        assert_eq!(prefs.shortcut_profiles.editor.active, "Custom (migrated)");
        assert_eq!(
            prefs.shortcuts,
            serde_json::from_str::<Value>(file).unwrap()["shortcuts"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                .collect()
        );
        assert_eq!(prefs.shortcut("tools.select.Brush", Some("B")), Some("Shift+B"));
        // Frozen pre-profile shape: serde's old reader ignores the new field and still
        // sees the exact legacy command map. This does not depend on the new resolver.
        #[derive(Deserialize)]
        struct OldPreferences {
            shortcuts: BTreeMap<String, String>,
        }
        let saved = session.prefs_to_json();
        let old: OldPreferences = serde_json::from_str(&saved).unwrap();
        assert_eq!(old.shortcuts, prefs.shortcuts);
        session.edit_prefs(|p| p.shortcut_profiles.editor.delete_active());
        let saved = session.prefs_to_json();
        session.load_prefs_json(&saved).unwrap();
        assert_eq!(session.prefs().shortcut_profiles.editor.active, "Photoshop", "stale legacy map must not re-migrate");
        assert_eq!(session.prefs().shortcuts, old.shortcuts);
    }

    #[test]
    fn keys_profiles_validation_paths_names_and_atomicity() {
        let mut session = Session::new();
        let mut profiles = ShortcutProfiles::default();
        profiles.migrate(&BTreeMap::new());
        profiles.library.active = "Capture One".into();
        profiles.library.duplicate("My C1.2").unwrap();
        session.execute("prefs.set", serde_json::json!({"path":"shortcutProfiles", "value":profiles})).unwrap();
        let path = "shortcutProfiles.library.custom.My C1.2.overrides.library::view.maskOverlay";
        session.execute("prefs.set", serde_json::json!({"path":path,"value":"shift+o"})).unwrap();
        assert_eq!(session.prefs().get(path), Some(serde_json::json!("Shift+O")));
        let before = session.prefs().clone();
        assert!(
            session
                .execute("prefs.set", serde_json::json!({"values": {"shortcutProfiles.editor.active":"Capture One","shortcuts":{"file.new":"F6"}}}))
                .is_err()
        );
        assert_eq!(session.prefs(), &before);
        assert!(session.execute("prefs.set", serde_json::json!({"path":path,"value":"Cmd+Banana"})).is_err());
        assert_eq!(session.prefs(), &before);
        session.edit_prefs(|p| {
            p.shortcut_profiles.library.rename("Sky").unwrap();
            p.shortcut_profiles.library.delete_active();
        });
        assert_eq!(session.prefs().shortcut_profiles.library.active, "Capture One");
        assert_eq!(session.prefs().shortcut_profiles.editor.active, "Photoshop");
    }

    #[test]
    fn keys_profiles_global_cannot_be_rebound_even_by_hold_keys() {
        let mut prefs = Preferences::default();
        prefs.shortcuts.insert("edit.undo".into(), "F6".into());
        prefs.shortcuts.insert("tools.temporary.hand".into(), "Cmd+Q".into());
        assert_eq!(prefs.shortcut("edit.undo", None), Some("Cmd+Z"));
        assert_eq!(prefs.shortcut("tools.temporary.hand", Some("Space")), None);
        assert!(global_shortcut_conflict("Cmd+Alt+1"));
        assert!(global_shortcut_conflict("Cmd+K"));
        assert!(!global_shortcut_conflict("Cmd+,"), "Library Settings and Photoshop layer visibility are workspace keys");
    }
}

#[cfg(test)]
mod j16_tests {
    use super::*;
    #[test]
    fn j16_library_and_ai_namespaces_round_trip_and_old_preferences_default() {
        let mut preferences = Preferences::default();
        preferences.set("library.settings.previewEdge", json!(3840)).unwrap();
        preferences.set("ai.comfy_host", json!("gpu.example")).unwrap();
        assert_eq!(preferences.get("library.settings.previewEdge"), Some(json!(3840)));
        assert_eq!(preferences.get("ai.comfy_host"), Some(json!("gpu.example")));
        let serialized = preferences.to_json();
        let restored: Preferences = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored, preferences);
        let old: Preferences = serde_json::from_value(json!({"general":{}})).unwrap();
        assert!(old.library.is_empty());
        assert_eq!(old.ai, li_ai::AiSettings::default());
    }
}
