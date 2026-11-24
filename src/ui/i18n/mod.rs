mod en;
mod locale;
mod message;
mod zh_cn;

pub use locale::Locale;
pub use message::{I18n, Message};

#[cfg(test)]
mod tests;
