use plato_core::document::{Document, Location};
use plato_core::document::html::HtmlDocument;
use plato_core::settings::ReaderSettings;

#[test]
fn reader_font_weight_config_is_backwards_compatible() {
    let defaults: ReaderSettings = toml::from_str("").unwrap();
    assert_eq!(defaults.font_weight, 0.0);
    for weight in [-0.5, 0.0, 1.5] {
        let settings: ReaderSettings = toml::from_str(&format!("font-weight = {weight}")).unwrap();
        assert_eq!(settings.font_weight, weight);
        let saved = toml::to_string(&settings).unwrap();
        let restored: ReaderSettings = toml::from_str(&saved).unwrap();
        assert_eq!(restored.font_weight, weight);
    }
}

#[test]
fn weight_changes_ink_without_changing_layout() {
    // Font and CSS resources are resolved relative to Plato's working directory.
    std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).unwrap();
    for family in ["serif", "sans-serif", "monospace"] {
        let html = format!("<html><body><p style=\"font-family: {family}\">The quick brown fox jumps over. Hamburgefontsiv 0123456789 fi fl.</p></body></html>");
        let mut doc = HtmlDocument::new_from_memory(&html);
        doc.layout(1072, 1448, 11.0, 300);
        let (original, _) = doc.pixmap(Location::Exact(0), 1.0, 1).unwrap();
        let words = format!("{:?}", doc.words(Location::Exact(0)));
        let ink = |pixels: &[u8]| pixels.iter().map(|&p| 255_u64 - p as u64).sum::<u64>();
        let original_ink = ink(original.data());
        for weight in [-0.5, 0.5] {
            doc.set_font_weight(weight);
            let (page, _) = doc.pixmap(Location::Exact(0), 1.0, 1).unwrap();
            assert_eq!(format!("{:?}", doc.words(Location::Exact(0))), words);
            if weight < 0.0 {
                assert!(ink(page.data()) < original_ink, "{family} should lighten");
            } else {
                assert!(ink(page.data()) > original_ink, "{family} should bolden");
            }
        }
        for weight in [0.0, f32::NAN, f32::INFINITY] {
            doc.set_font_weight(weight);
            let (page, _) = doc.pixmap(Location::Exact(0), 1.0, 1).unwrap();
            assert_eq!(page.data(), original.data(), "neutral weight must preserve pixels");
        }
    }
}

#[test]
fn book_and_theme_weight_round_trip() {
    use plato_core::metadata::ReaderInfo;
    use plato_core::settings::Theme;
    let mut book = ReaderInfo::default();
    assert_eq!(book.font_weight, None);
    book.font_weight = Some(-1.5);
    let restored: ReaderInfo = serde_json::from_str(&serde_json::to_string(&book).unwrap()).unwrap();
    assert_eq!(restored.font_weight, Some(-1.5));
    let old: Theme = toml::from_str("name = 'Old'\nfont-size = 11.0").unwrap();
    assert_eq!(old.font_weight, None);
    let theme: Theme = toml::from_str("name = 'Weight'\nfont-weight = 2.5").unwrap();
    let restored: Theme = toml::from_str(&toml::to_string(&theme).unwrap()).unwrap();
    assert_eq!(restored.font_weight, Some(2.5));
}

#[test]
fn overwrite_theme_selects_present_fields_even_when_false_or_zero() {
    use plato_core::settings::{ReaderSettings, Theme};
    use plato_core::view::theme::{initial_selection, ThemeProp};
    let theme: Theme = toml::from_str("name = 'Adele'\nfont-family = 'Adele'\nfont-size = 18.365675\nfont-weight = 1.0\ninverted = true").unwrap();
    assert_eq!(initial_selection(Some(&theme), None, &ReaderSettings::default()), vec![ThemeProp::FontFamily, ThemeProp::FontSize, ThemeProp::FontWeight, ThemeProp::InvertedMode]);
    let theme: Theme = toml::from_str("name = 'Neutral'\nfont-weight = 0.0\ninverted = false\nfrontlight = false").unwrap();
    assert_eq!(initial_selection(Some(&theme), None, &ReaderSettings::default()), vec![ThemeProp::FontWeight, ThemeProp::FrontLight, ThemeProp::InvertedMode]);
    let theme: Theme = toml::from_str("name = 'Relative'\nfont-size = 2.0\nfont-size-relative = true").unwrap();
    assert_eq!(initial_selection(Some(&theme), None, &ReaderSettings::default()), vec![ThemeProp::RelativeFontSize]);
}

#[test]
fn new_theme_selects_only_reader_values_differing_from_defaults() {
    use plato_core::settings::ReaderSettings;
    use plato_core::metadata::ReaderInfo;
    use plato_core::view::theme::{initial_selection, ThemeProp};
    let defaults = ReaderSettings::default();
    assert!(initial_selection(None, None, &defaults).is_empty());
    let mut reader = ReaderInfo::default();
    reader.font_family = Some(defaults.font_family.clone());
    reader.font_size = Some(defaults.font_size);
    reader.font_weight = Some(defaults.font_weight);
    assert!(initial_selection(None, Some(&reader), &defaults).is_empty());
    reader.font_size = Some(defaults.font_size + 1.0);
    reader.font_weight = Some(-0.5);
    assert_eq!(initial_selection(None, Some(&reader), &defaults), vec![ThemeProp::FontSize, ThemeProp::FontWeight]);
}
