use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

use jarvis_core::{i18n, voices, SettingsManager};

// RADIO GROUP

// a group of check menu items where only one can be active at a time.
// stores (menu_item, setting_value) pairs.
pub struct RadioGroup {
    pub setting_key: String,
    pub items: Vec<(CheckMenuItem, String)>,
}

impl RadioGroup {
    pub fn select(&self, value: &str) {
        for (item, val) in &self.items {
            item.set_checked(val == value);
        }
    }
}

// TRAY MENU STATE

pub struct TrayMenu {
    pub menu: Menu,
    pub state: TrayState,
}

// holds references to menu items for updating check marks after build
pub struct TrayState {
    pub radio_groups: Vec<RadioGroup>,
    pub gain_toggle: CheckMenuItem,
}

// BUILD

pub fn build(settings: &SettingsManager) -> TrayMenu {
    let menu = Menu::new();

    let mut radio_groups = Vec::new();

    // -- language submenu
    let lang_sub = Submenu::new(i18n::t("tray-language"), true);
    let current_lang = settings.read("language").unwrap_or_default();
    let mut lang_items = Vec::new();
    for &lang in i18n::SUPPORTED_LANGUAGES {
        let label = match lang {
            "ru" => "Русский",
            "en" => "English",
            "ua" => "Українська",
            _ => lang,
        };
        let item = CheckMenuItem::with_id(
            format!("set:language:{}", lang),
            label,
            true,
            lang == current_lang,
            None,
        );
        let _ = lang_sub.append(&item);
        lang_items.push((item, lang.to_string()));
    }
    radio_groups.push(RadioGroup {
        setting_key: "language".to_string(),
        items: lang_items,
    });

    // -- voice submenu
    let voice_sub = Submenu::new(i18n::t("tray-voice"), true);
    let current_voice = voices::get_current_voice()
        .map(|v| v.voice.id.clone())
        .unwrap_or_default();
    let mut voice_items = Vec::new();
    for voice in voices::list_voices() {
        let item = CheckMenuItem::with_id(
            format!("set:assistant_voice:{}", voice.voice.id),
            &voice.voice.name,
            true,
            voice.voice.id == current_voice,
            None,
        );
        let _ = voice_sub.append(&item);
        voice_items.push((item, voice.voice.id.clone()));
    }
    radio_groups.push(RadioGroup {
        setting_key: "assistant_voice".to_string(),
        items: voice_items,
    });

    // -- wake word engine submenu
    let ww_sub = Submenu::new(i18n::t("tray-wake-word"), true);
    let current_ww = settings.read("selected_wake_word_engine").unwrap_or_default();
    let mut ww_items = Vec::new();
    for (label, value) in &[("Rustpotter", "Rustpotter"), ("Vosk", "Vosk")] {
        let item = CheckMenuItem::with_id(
            format!("set:selected_wake_word_engine:{}", value.to_lowercase()),
            *label,
            true,
            current_ww == *label,
            None,
        );
        let _ = ww_sub.append(&item);
        ww_items.push((item, value.to_lowercase()));
    }
    radio_groups.push(RadioGroup {
        setting_key: "selected_wake_word_engine".to_string(),
        items: ww_items,
    });

    // -- noise suppression submenu
    let ns_sub = Submenu::new(i18n::t("tray-noise-suppression"), true);
    let current_ns = settings.read("noise_suppression").unwrap_or_default();
    let mut ns_items = Vec::new();
    for (label, value) in &[("None", "none"), ("Nnnoiseless", "nnnoiseless")] {
        let item = CheckMenuItem::with_id(
            format!("set:noise_suppression:{}", value),
            *label,
            true,
            current_ns.to_lowercase() == *value,
            None,
        );
        let _ = ns_sub.append(&item);
        ns_items.push((item, value.to_string()));
    }
    radio_groups.push(RadioGroup {
        setting_key: "noise_suppression".to_string(),
        items: ns_items,
    });

    // -- vad submenu
    let vad_sub = Submenu::new(i18n::t("tray-vad"), true);
    let current_vad = settings.read("vad_backend").unwrap_or_default();
    let mut vad_items = Vec::new();
    for (label, value) in &[("None", "none"), ("Energy", "energy"), ("Nnnoiseless", "nnnoiseless")] {
        let item = CheckMenuItem::with_id(
            format!("set:vad_backend:{}", value),
            *label,
            true,
            current_vad == *value,
            None,
        );
        let _ = vad_sub.append(&item);
        vad_items.push((item, value.to_string()));
    }
    radio_groups.push(RadioGroup {
        setting_key: "vad_backend".to_string(),
        items: vad_items,
    });

    // -- gain normalizer toggle
    let gain_on = settings.read("gain_normalizer")
        .map(|v| v == "true")
        .unwrap_or(true);
    let gain_toggle = CheckMenuItem::with_id(
        "toggle:gain_normalizer",
        i18n::t("tray-gain-normalizer"),
        true,
        gain_on,
        None,
    );

    // -- assemble main menu
    let _ = menu.append(&lang_sub);
    let _ = menu.append(&voice_sub);
    let _ = menu.append(&ww_sub);
    let _ = menu.append(&ns_sub);
    let _ = menu.append(&vad_sub);
    let _ = menu.append(&gain_toggle);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id("restart", i18n::t("tray-restart"), true, None));
    let _ = menu.append(&MenuItem::with_id("settings", i18n::t("tray-settings"), true, None));
    let _ = menu.append(&MenuItem::with_id("exit", i18n::t("tray-exit"), true, None));

    TrayMenu {
        menu,
        state: TrayState {
            radio_groups,
            gain_toggle,
        },
    }
}
