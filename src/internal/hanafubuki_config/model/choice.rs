//! The spelling of a settings choice, written once per enum.
//!
//! A choice is stored, patched and offered by the settings schema under the
//! same names. [`setting_choice!`] declares them beside the enum: the variant
//! list the schema offers (`ALL`, in display order), the stored spelling
//! (`as_str`, an exhaustive match) and the parse (`TryFrom<&str>`). The enum's
//! serde attributes spell the same names; the settings tests hold the two to
//! each other.

macro_rules! setting_choice {
    ($choice:ident, $field:literal, { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $choice {
            /// Every choice, in the order the settings schema offers them.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The name the choice is stored and patched under.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }

        impl TryFrom<&str> for $choice {
            type Error = String;

            fn try_from(value: &str) -> Result<Self, String> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|choice| choice.as_str() == value)
                    .ok_or_else(|| format!(concat!($field, " has unsupported value: {}"), value))
            }
        }
    };
}

pub(crate) use setting_choice;
