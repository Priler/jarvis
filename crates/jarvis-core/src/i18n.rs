use fluent_bundle::{FluentResource, FluentArgs, FluentValue};
use fluent_bundle::concurrent::FluentBundle as ConcurrentFluentBundle;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use std::collections::HashMap;
use unic_langid::LanguageIdentifier;

// locale files embedded at compile time
const LOCALE_RU: &str = include_str!("i18n/locales/ru.ftl");
const LOCALE_EN: &str = include_str!("i18n/locales/en.ftl");
const LOCALE_UA: &str = include_str!("i18n/locales/ua.ftl");

pub const SUPPORTED_LANGUAGES: &[&str] = &["ru", "en", "ua"];
pub const DEFAULT_LANGUAGE: &str = "en";

// detect the OS language and map it to a supported language.
// falls back to DEFAULT_LANGUAGE if not supported.
pub fn detect_system_language() -> &'static str {
    if let Some(locale) = sys_locale::get_locale() {
        // locale can be "en-US", "ru-RU", "uk-UA", etc.
        let lang_code = locale.split(&['-', '_'][..]).next().unwrap_or("");

        // map OS locale codes to our supported languages
        let mapped = match lang_code {
            "uk" => "ua",  // ISO 639-1 "uk" (ukrainian) -> our "ua"
            other => other,
        };

        if SUPPORTED_LANGUAGES.contains(&mapped) {
            info!("Detected system language: {} (from locale '{}')", mapped, locale);
            return SUPPORTED_LANGUAGES.iter()
                .find(|&&l| l == mapped)
                .unwrap();
        }

        info!("System locale '{}' not supported, using default '{}'", locale, DEFAULT_LANGUAGE);
    }

    DEFAULT_LANGUAGE
}

// use concurrent bundle (thread-safe)
type Bundle = ConcurrentFluentBundle<FluentResource>;

static BUNDLES: OnceCell<HashMap<String, Bundle>> = OnceCell::new();
static CURRENT_LANG: OnceCell<RwLock<String>> = OnceCell::new();

// Initialize i18n system
pub fn init(lang: &str) {
    let bundles = create_bundles();
    BUNDLES.set(bundles).ok();
    
    let lang = if SUPPORTED_LANGUAGES.contains(&lang) { lang } else { DEFAULT_LANGUAGE };
    CURRENT_LANG.set(RwLock::new(lang.to_string())).ok();
    
    info!("i18n initialized with language: {}", lang);
}

fn create_bundles() -> HashMap<String, Bundle> {
    let mut bundles = HashMap::new();
    
    bundles.insert("ru".to_string(), create_bundle("ru", LOCALE_RU));
    bundles.insert("en".to_string(), create_bundle("en", LOCALE_EN));
    bundles.insert("ua".to_string(), create_bundle("ua", LOCALE_UA));
    
    bundles
}

fn create_bundle(lang: &str, source: &str) -> Bundle {
    let langid: LanguageIdentifier = lang.parse().expect("Invalid language identifier");
    let mut bundle = ConcurrentFluentBundle::new_concurrent(vec![langid]);
    
    let resource = FluentResource::try_new(source.to_string())
        .expect("Failed to parse FTL resource");
    
    bundle.add_resource(resource).expect("Failed to add resource");
    bundle
}

// Set current language
pub fn set_language(lang: &str) {
    if let Some(current) = CURRENT_LANG.get() {
        let lang = if SUPPORTED_LANGUAGES.contains(&lang) { lang } else { DEFAULT_LANGUAGE };
        *current.write() = lang.to_string();
        info!("Language changed to: {}", lang);
    }
}

// Get current language
pub fn get_language() -> String {
    CURRENT_LANG.get()
        .map(|l| l.read().clone())
        .unwrap_or_else(|| DEFAULT_LANGUAGE.to_string())
}

// Translate a key
pub fn t(key: &str) -> String {
    t_with_args(key, None)
}

// Translate a key with arguments
pub fn t_with_args(key: &str, args: Option<&FluentArgs>) -> String {
    let lang = get_language();
    
    let bundles = match BUNDLES.get() {
        Some(b) => b,
        None => return key.to_string(),
    };
    
    let bundle = match bundles.get(&lang) {
        Some(b) => b,
        None => bundles.get(DEFAULT_LANGUAGE).unwrap(),
    };
    
    let msg = match bundle.get_message(key) {
        Some(m) => m,
        None => return key.to_string(),
    };
    
    let pattern = match msg.value() {
        Some(p) => p,
        None => return key.to_string(),
    };
    
    let mut errors = vec![];
    let result = bundle.format_pattern(pattern, args, &mut errors);
    
    if !errors.is_empty() {
        warn!("i18n errors for key '{}': {:?}", key, errors);
    }
    
    result.to_string()
}

// Translate with a single argument
pub fn t_arg(key: &str, arg_name: &str, arg_value: &str) -> String {
    let mut args = FluentArgs::new();
    args.set(arg_name, FluentValue::from(arg_value));
    t_with_args(key, Some(&args))
}

// Translate with numeric argument
pub fn t_count(key: &str, count: i64) -> String {
    let mut args = FluentArgs::new();
    args.set("count", FluentValue::from(count));
    t_with_args(key, Some(&args))
}

// Get all translations for current language (for frontend)
pub fn get_all_translations() -> HashMap<String, String> {
    let lang = get_language();
    get_translations_for(&lang)
}

// Get all translations for a specific language
pub fn get_translations_for(lang: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    
    let bundles = match BUNDLES.get() {
        Some(b) => b,
        None => return result,
    };
    
    let bundle = match bundles.get(lang) {
        Some(b) => b,
        None => match bundles.get(DEFAULT_LANGUAGE) {
            Some(b) => b,
            None => return result,
        },
    };

    // get source for this language and extract all keys
    let source = match lang {
        "ru" => LOCALE_RU,
        "en" => LOCALE_EN,
        "ua" => LOCALE_UA,
        _ => LOCALE_RU,
    };

    // parse keys from FTL source (lines that have "=" and don't start with "#" or "-")
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        
        if let Some(key) = line.split('=').next() {
            let key = key.trim();
            if !key.is_empty() && !key.contains(' ') {
                if let Some(msg) = bundle.get_message(key) {
                    if let Some(pattern) = msg.value() {
                        let mut errors = vec![];
                        let value = bundle.format_pattern(pattern, None, &mut errors);
                        result.insert(key.to_string(), value.to_string());
                    }
                }
            }
        }
    }
    
    result
}