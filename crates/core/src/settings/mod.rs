mod preset;

use std::env;
use std::ops::Index;
use std::fmt::{self, Debug};
use std::path::PathBuf;
use std::collections::{BTreeMap, HashMap};
use fxhash::FxHashSet;
use serde::{Serialize, Deserialize};
use sys_locale::get_locale;
use crate::metadata::{SortMethod, TextAlign};
use crate::frontlight::LightLevels;
use crate::color::BLACK;
use crate::device::CURRENT_DEVICE;
use crate::unit::mm_to_px;

pub use self::preset::{LightPreset, guess_frontlight};

pub const SETTINGS_PATH: &str = "Settings.toml";
pub const DEFAULT_FONT_PATH: &str = "/mnt/onboard/fonts";
pub const INTERNAL_CARD_ROOT: &str = "/mnt/onboard";
pub const EXTERNAL_CARD_ROOT: &str = "/mnt/sd";
pub const LOGO_SPECIAL_PATH: &str = "logo:";
pub const COVER_SPECIAL_PATH: &str = "cover:";
// Default font size in points.
pub const DEFAULT_FONT_SIZE: f32 = 11.0;
// Default margin width in millimeters.
pub const DEFAULT_MARGIN_WIDTH: i32 = 8;
// Default line height in ems.
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;
// Default font family name.
pub const DEFAULT_FONT_FAMILY: &str = "Libertinus Serif";
// Default text alignment.
pub const DEFAULT_TEXT_ALIGN: TextAlign = TextAlign::Left;
pub const HYPHEN_PENALTY: i32 = 50;
pub const STRETCH_TOLERANCE: f32 = 1.26;
pub const MIN_LINE_HEIGHT_GRADIENT: f32 = 0.0125;
pub const MAX_LINE_HEIGHT_GRADIENT: f32 = 0.25;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RotationLock {
    Landscape,
    Portrait,
    Current,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonScheme {
    Natural,
    Inverted,
}

impl fmt::Display for ButtonScheme {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntermKind {
    Suspend,
    PowerOff,
    Share,
}

impl IntermKind {
    pub fn text(&self) -> &str {
        match self {
            IntermKind::Suspend => "Sleeping",
            IntermKind::PowerOff => "Powered off",
            IntermKind::Share => "Shared",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Intermissions {
    suspend: PathBuf,
    power_off: PathBuf,
    share: PathBuf,
}

impl Index<IntermKind> for Intermissions {
    type Output = PathBuf;

    fn index(&self, key: IntermKind) -> &Self::Output {
        match key {
            IntermKind::Suspend => &self.suspend,
            IntermKind::PowerOff => &self.power_off,
            IntermKind::Share => &self.share,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub selected_library: usize,
    pub keyboard_layout: String,
    pub frontlight: bool,
    pub wifi: bool,
    pub inverted: bool,
    pub sleep_cover: bool,
    pub auto_share: bool,
    pub suppress_screen_flash: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_lock: Option<RotationLock>,
    pub button_scheme: ButtonScheme,
    pub auto_suspend: f32,
    pub auto_power_off: f32,
    pub time_format: String,
    pub date_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_urls_queue: Option<PathBuf>,
    pub max_warmth: f32,
    pub google_translate_server: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_to_library: Option<String>,
    pub wikipedia_languages: Vec<String>,
    pub languages: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<LibrarySettings>,
    pub intermissions: Intermissions,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frontlight_presets: Vec<LightPreset>,
    pub home: HomeSettings,
    pub reader: ReaderSettings,
    pub import: ImportSettings,
    pub dictionary: DictionarySettings,
    pub sketch: SketchSettings,
    pub calculator: CalculatorSettings,
    pub battery: BatterySettings,
    pub frontlight_levels: LightLevels,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<Theme>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub css_styles: Vec<CssStyle>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryMode {
    Database,
    Filesystem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LibrarySettings {
    pub name: String,
    pub path: PathBuf,
    pub mode: LibraryMode,
    pub sort_method: SortMethod,
    pub first_column: FirstColumn,
    pub second_column: SecondColumn,
    pub thumbnail_previews: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        LibrarySettings {
            name: "Unnamed".to_string(),
            path: env::current_dir().ok()
                      .unwrap_or_else(|| PathBuf::from("/")),
            mode: LibraryMode::Database,
            sort_method: SortMethod::Status,
            first_column: FirstColumn::TitleAndAuthor,
            second_column: SecondColumn::Progress,
            thumbnail_previews: true,
            hooks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Theme {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_relative: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_document_css: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inverted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontlight: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontlight_levels: Option<LightLevels>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismiss: Option<bool>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            name: "Unnamed".to_string(),
            font_family: None,
            font_size: None,
            font_size_relative: None,
            text_align: None,
            margin_width: None,
            line_height: None,
            ignore_document_css: None,
            inverted: None,
            frontlight: None,
            frontlight_levels: None,
            dismiss: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CssStyle {
    pub name: String,
    pub css: String,
}

impl Default for CssStyle {
    fn default() -> Self {
        CssStyle {
            name: "Unnamed".to_string(),
            css: "".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ImportSettings {
    pub unshare_trigger: bool,
    pub startup_trigger: bool,
    pub sync_metadata: bool,
    pub metadata_kinds: FxHashSet<String>,
    pub allowed_kinds: FxHashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DictionarySettings {
    pub margin_width: i32,
    pub font_size: f32,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub languages: BTreeMap<String, Vec<String>>,
}

impl Default for DictionarySettings {
    fn default() -> Self {
        DictionarySettings {
            font_size: 11.0,
            margin_width: 4,
            languages: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SketchSettings {
    pub save_path: PathBuf,
    pub notify_success: bool,
    pub pen: Pen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CalculatorSettings {
    pub font_size: f32,
    pub margin_width: i32,
    pub history_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Pen {
    pub size: i32,
    pub color: u8,
    pub dynamic: bool,
    pub amplitude: f32,
    pub min_speed: f32,
    pub max_speed: f32,
}

impl Default for Pen {
    fn default() -> Self {
        Pen {
            size: 2,
            color: BLACK,
            dynamic: true,
            amplitude: 4.0,
            min_speed: 0.0,
            max_speed: mm_to_px(254.0, CURRENT_DEVICE.dpi),
        }
    }
}

impl Default for SketchSettings {
    fn default() -> Self {
        SketchSettings {
            save_path: PathBuf::from("Sketches"),
            notify_success: true,
            pen: Pen::default(),
        }
    }
}

impl Default for CalculatorSettings {
    fn default() -> Self {
        CalculatorSettings {
            font_size: 8.0,
            margin_width: 2,
            history_size: 4096,
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Columns {
    first: FirstColumn,
    second: SecondColumn,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FirstColumn {
    TitleAndAuthor,
    FileName,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecondColumn {
    Progress,
    Year,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Hook {
    pub path: PathBuf,
    pub program: PathBuf,
    pub sort_method: Option<SortMethod>,
    pub first_column: Option<FirstColumn>,
    pub second_column: Option<SecondColumn>,
}

impl Default for Hook {
    fn default() -> Self {
        Hook {
            path: PathBuf::default(),
            program: PathBuf::default(),
            sort_method: None,
            first_column: None,
            second_column: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HomeSettings {
    pub address_bar: bool,
    pub navigation_bar: bool,
    pub max_levels: usize,
    pub max_trash_size: u64,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct RefreshRateSettings {
    #[serde(flatten)]
    pub global: RefreshRatePair,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub by_kind: HashMap<String, RefreshRatePair>,
    pub chapter_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RefreshRatePair {
    pub regular: u8,
    pub inverted: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ProgressBarSettings {
    pub enabled: bool,
    pub height: i32,
    pub horz_margin: i32,
    pub vert_margin: i32,
    pub show_clock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ReaderSettings {
    pub finished: FinishedAction,
    pub south_east_corner: SouthEastCornerAction,
    pub bottom_right_gesture: BottomRightGestureAction,
    pub south_strip: SouthStripAction,
    pub west_strip: WestStripAction,
    pub east_strip: EastStripAction,
    pub strip_width: f32,
    pub corner_width: f32,
    pub font_path: String,
    pub font_family: String,
    pub font_size: f32,
    pub min_font_size: f32,
    pub max_font_size: f32,
    pub text_align: TextAlign,
    pub margin_width: i32,
    pub min_margin_width: i32,
    pub max_margin_width: i32,
    pub line_height: f32,
    pub continuous_fit_to_width: bool,
    pub line_height_gradient: f32,
    pub ignore_document_css: bool,
    pub dithered_kinds: FxHashSet<String>,
    pub paragraph_breaker: ParagraphBreakerSettings,
    pub refresh_rate: RefreshRateSettings,
    pub progress_bar: ProgressBarSettings,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ParagraphBreakerSettings {
    pub hyphen_penalty: i32,
    pub stretch_tolerance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BatterySettings {
    pub warn: f32,
    pub power_off: f32,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FinishedAction {
    Notify,
    Close,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SouthEastCornerAction {
    NextPage,
    GoToPage,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BottomRightGestureAction {
    ToggleDithered,
    ToggleInverted,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SouthStripAction {
    ToggleBars,
    NextPage,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EastStripAction {
    PreviousPage,
    NextPage,
    None,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WestStripAction {
    PreviousPage,
    NextPage,
    None,
}

impl Default for RefreshRateSettings {
    fn default() -> Self {
        RefreshRateSettings {
            global: RefreshRatePair { regular: 8, inverted: 2 },
            by_kind: HashMap::new(),
            chapter_start: true,
        }
    }
}

impl Default for ProgressBarSettings {
    fn default() -> Self {
        ProgressBarSettings {
            enabled: true,
            height: 12,
            horz_margin: 20,
            vert_margin: 12,
            show_clock: true,
        }
    }
}

impl Default for HomeSettings {
    fn default() -> Self {
        HomeSettings {
            address_bar: false,
            navigation_bar: true,
            max_levels: 3,
            max_trash_size: 32 * (1 << 20),
        }
    }
}

impl Default for ParagraphBreakerSettings {
    fn default() -> Self {
        ParagraphBreakerSettings {
            hyphen_penalty: HYPHEN_PENALTY,
            stretch_tolerance: STRETCH_TOLERANCE,
        }
    }
}

impl Default for ReaderSettings {
    fn default() -> Self {
        ReaderSettings {
            finished: FinishedAction::Close,
            south_east_corner: SouthEastCornerAction::GoToPage,
            bottom_right_gesture: BottomRightGestureAction::ToggleDithered,
            south_strip: SouthStripAction::ToggleBars,
            west_strip: WestStripAction::PreviousPage,
            east_strip: EastStripAction::NextPage,
            strip_width: 0.6,
            corner_width: 0.4,
            font_path: DEFAULT_FONT_PATH.to_string(),
            font_family: DEFAULT_FONT_FAMILY.to_string(),
            font_size: DEFAULT_FONT_SIZE,
            min_font_size: DEFAULT_FONT_SIZE / 2.0,
            max_font_size: 3.0 * DEFAULT_FONT_SIZE / 2.0,
            text_align: DEFAULT_TEXT_ALIGN,
            margin_width: DEFAULT_MARGIN_WIDTH,
            min_margin_width: DEFAULT_MARGIN_WIDTH.saturating_sub(8),
            max_margin_width: DEFAULT_MARGIN_WIDTH.saturating_add(2),
            line_height: DEFAULT_LINE_HEIGHT,
            continuous_fit_to_width: true,
            line_height_gradient: 0.1,
            ignore_document_css: false,
            dithered_kinds: ["cbz", "png", "jpg", "jpeg"].iter().map(|k| k.to_string()).collect(),
            paragraph_breaker: ParagraphBreakerSettings::default(),
            refresh_rate: RefreshRateSettings::default(),
            progress_bar: ProgressBarSettings::default(),
        }
    }
}

impl Default for ImportSettings {
    fn default() -> Self {
        ImportSettings {
            unshare_trigger: true,
            startup_trigger: true,
            sync_metadata: true,
            metadata_kinds: ["epub", "pdf", "djvu"].iter().map(|k| k.to_string()).collect(),
            allowed_kinds: ["pdf", "djvu", "epub", "fb2",
                            "xps", "oxps", "mobi", "cbz"].iter().map(|k| k.to_string()).collect(),
        }
    }
}

impl Default for BatterySettings {
    fn default() -> Self {
        BatterySettings {
            warn: 10.0,
            power_off: 3.0,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            selected_library: 0,
            libraries: vec![
                LibrarySettings {
                    name: "On Board".to_string(),
                    path: PathBuf::from(INTERNAL_CARD_ROOT),
                    hooks: vec![
                        Hook {
                            path: PathBuf::from("Articles"),
                            program: PathBuf::from("bin/article_fetcher/article_fetcher"),
                            sort_method: Some(SortMethod::Added),
                            first_column: Some(FirstColumn::TitleAndAuthor),
                            second_column: Some(SecondColumn::Progress),
                        }
                    ],
                    .. Default::default()
                },
                LibrarySettings {
                    name: "Removable".to_string(),
                    path: PathBuf::from(EXTERNAL_CARD_ROOT),
                    .. Default::default()
                },
                LibrarySettings {
                    name: "Dropbox".to_string(),
                    path: PathBuf::from("/mnt/onboard/.kobo/dropbox"),
                    .. Default::default()
                },
                LibrarySettings {
                    name: "KePub".to_string(),
                    path: PathBuf::from("/mnt/onboard/.kobo/kepub"),
                    .. Default::default()
                },
            ],
            external_urls_queue: Some(PathBuf::from("bin/article_fetcher/urls.txt")),
            keyboard_layout: "English".to_string(),
            frontlight: true,
            wifi: false,
            inverted: false,
            sleep_cover: true,
            auto_share: false,
            suppress_screen_flash: true,
            rotation_lock: None,
            button_scheme: ButtonScheme::Natural,
            auto_suspend: 30.0,
            auto_power_off: 3.0,
            time_format: "%H:%M".to_string(),
            date_format: "%A, %B %-d, %Y".to_string(),
            intermissions: Intermissions {
                suspend: PathBuf::from(COVER_SPECIAL_PATH),
                power_off: PathBuf::from(LOGO_SPECIAL_PATH),
                share: PathBuf::from(LOGO_SPECIAL_PATH),
            },
            home: HomeSettings::default(),
            reader: ReaderSettings::default(),
            import: ImportSettings::default(),
            dictionary: DictionarySettings::default(),
            sketch: SketchSettings::default(),
            calculator: CalculatorSettings::default(),
            battery: BatterySettings::default(),
            frontlight_levels: LightLevels::default(),
            frontlight_presets: Vec::new(),
            max_warmth: 100.0,
            google_translate_server: "https://translate.googleapis.com".to_string(),
            save_to_library: None,
            wikipedia_languages: vec![String::from("en")],
            languages: vec![get_locale().unwrap_or_else(|| String::from("en"))],
            themes: vec![
                Theme {
                    name: "Comfy".to_string(),
                    font_size: Some(15.0),
                    line_height: Some(1.25),
                    margin_width: Some(6),
                    .. Default::default()
                },
                Theme {
                    name: "Compact".to_string(),
                    font_size: Some(13.0),
                    line_height: Some(1.1),
                    margin_width: Some(3),
                    .. Default::default()
                },
                Theme {
                    name: "Bed time".to_string(),
                    font_size: Some(14.0),
                    inverted: Some(false),
                    frontlight: Some(true),
                    frontlight_levels: Some(
                        LightLevels {
                            intensity: 10.0,
                            warmth: 50.0,
                        }
                    ),
                    .. Default::default()
                },
            ],
            css_styles: vec![
                CssStyle {
                    name: "Main paragraph".to_string(),
                    css: "margin:0; padding:0; text-indent:1.5em; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;".to_string(),
                },
                CssStyle {
                    name: "Opening paragraph".to_string(),
                    css: "margin:2em 0 0 0; padding:0; text-indent:0; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;".to_string(),
                },
                CssStyle {
                    name: "Force preferences".to_string(),
                    css: "font-family:serif; text-align:%textalign%; font-size:%fontsize%; line-height:%lineheight%;".to_string(),
                },
            ],
        }
    }
}
