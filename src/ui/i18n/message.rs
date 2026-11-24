use crate::workflow::{LauncherFailure, LauncherFileOperation, LauncherPhase};

use super::{Locale, en, zh_cn};

pub enum Message<'a> {
    RetryingPortableSetup,
    PreparationFailed,
    Retry,
    Exit,
    DownloadingHanafubuki {
        version: &'a str,
    },
    UpdatingLauncher {
        version: &'a str,
    },
    ScanProgress {
        files: u64,
        directories: u64,
        webuis: u64,
        current_path: &'a str,
    },
    DownloadProgress {
        completed: u64,
        total: Option<u64>,
    },
    FileProgress {
        operation: LauncherFileOperation,
        completed: u64,
        total: Option<u64>,
        current_path: &'a str,
    },
}

pub struct PhaseText {
    pub title: &'static str,
    pub detail: &'static str,
}

#[derive(Clone, Copy)]
pub struct I18n {
    locale: Locale,
}

impl I18n {
    pub fn from_system() -> Self {
        Self::new(Locale::detect(sys_locale::get_locale().as_deref()))
    }

    pub const fn new(locale: Locale) -> Self {
        Self { locale }
    }

    pub fn t(self, message: Message<'_>) -> String {
        match self.locale {
            Locale::English => en::message(message),
            Locale::SimplifiedChinese => zh_cn::message(message),
        }
    }

    pub fn phase(self, phase: LauncherPhase) -> PhaseText {
        match self.locale {
            Locale::English => en::phase(phase),
            Locale::SimplifiedChinese => zh_cn::phase(phase),
        }
    }

    pub fn failure(self, failure: &LauncherFailure) -> String {
        match self.locale {
            Locale::English => en::failure(failure),
            Locale::SimplifiedChinese => zh_cn::failure(failure),
        }
    }
}

pub(super) fn mib(value: u64) -> f64 {
    value as f64 / (1024.0 * 1024.0)
}
