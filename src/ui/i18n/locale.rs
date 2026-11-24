#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    English,
    SimplifiedChinese,
}

impl Locale {
    pub fn detect(system_locale: Option<&str>) -> Self {
        let Some(locale) = system_locale else {
            return Self::English;
        };
        let locale = locale.to_ascii_lowercase();
        if locale == "zh" || locale.starts_with("zh-") || locale.starts_with("zh_") {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }
}
