//! Explicit UI translations. User clock labels, zone IDs and content templates are never looked up.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        LazyLock,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Language {
    #[default]
    English,
    Russian,
    French,
    German,
    Spanish,
    Italian,
}

pub const LANGUAGES: [Language; 6] = [
    Language::English,
    Language::Russian,
    Language::French,
    Language::German,
    Language::Spanish,
    Language::Italian,
];
static CURRENT: AtomicU8 = AtomicU8::new(0);
static CATALOGS: LazyLock<[BTreeMap<String, String>; 6]> = LazyLock::new(|| {
    [
        include_str!("../locales/en.json"),
        include_str!("../locales/ru.json"),
        include_str!("../locales/fr.json"),
        include_str!("../locales/de.json"),
        include_str!("../locales/es.json"),
        include_str!("../locales/it.json"),
    ]
    .map(|source| serde_json::from_str(source).expect("validated bundled UI catalog"))
});

impl Language {
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }
    pub fn translate(self, text: &str) -> &str {
        CATALOGS[self as usize]
            .get(text)
            .map_or(text, String::as_str)
    }
}

pub fn set_language(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}
pub fn language() -> Language {
    LANGUAGES[usize::from(CURRENT.load(Ordering::Relaxed)).min(5)]
}
pub fn tr(text: &str) -> &str {
    language().translate(text)
}
pub fn message(key: &str, arguments: &[(&str, String)]) -> String {
    let mut result = tr(key).to_owned();
    for (name, value) in arguments {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(text: &str) -> Vec<&str> {
        let mut values: Vec<_> = text
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .collect();
        values.sort_unstable();
        values
    }
    #[test]
    fn every_language_is_complete_and_preserves_message_parameters() {
        for language in LANGUAGES {
            let catalog = &CATALOGS[language as usize];
            assert_eq!(catalog.len(), CATALOGS[0].len(), "{language:?}");
            for (key, english) in &CATALOGS[0] {
                let translated = catalog
                    .get(key)
                    .unwrap_or_else(|| panic!("{language:?}: {key}"));
                assert!(!translated.trim().is_empty());
                assert_eq!(
                    placeholders(english),
                    placeholders(translated),
                    "{language:?}: {key}"
                );
            }
        }
        assert_eq!(
            Language::Russian.translate("An unknown diagnostic"),
            "An unknown diagnostic"
        );
    }
}
