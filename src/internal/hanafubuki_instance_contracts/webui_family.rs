use serde::{Deserialize, Serialize};

/// The WebUI families the upstream `sd_webui_all_in_one` library manages.
///
/// This is the single family identity shared by every instance domain. It
/// carries only the names the upstream library addresses a family by; which
/// operations and parameters a family supports belong to that family's module
/// in the owning domain, reached through an exhaustive `match` on this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebUiFamily {
    StableDiffusionWebui,
    Comfyui,
    Invokeai,
    Fooocus,
    SdTrainer,
    QwenTts,
    RvcNext,
    MizuotoTts,
}

impl WebUiFamily {
    pub const ALL: [Self; 8] = [
        Self::StableDiffusionWebui,
        Self::Comfyui,
        Self::Invokeai,
        Self::Fooocus,
        Self::SdTrainer,
        Self::QwenTts,
        Self::RvcNext,
        Self::MizuotoTts,
    ];

    /// Product name used when composing a default instance name. Deliberately
    /// backend-owned: the generated name is persisted state, so it must not
    /// depend on the frontend's locale or display strings.
    pub fn product_name(self) -> &'static str {
        match self {
            Self::StableDiffusionWebui => "Stable Diffusion WebUI",
            Self::Comfyui => "ComfyUI",
            Self::Invokeai => "InvokeAI",
            Self::Fooocus => "Fooocus",
            Self::SdTrainer => "SD Trainer",
            Self::QwenTts => "Qwen TTS",
            Self::RvcNext => "RVC Next",
            Self::MizuotoTts => "Mizuoto TTS",
        }
    }

    /// Namespace of the family's App API methods, e.g. `comfyui` in
    /// `comfyui.launch.prepare`. Upstream snapshots record the same name.
    pub fn api_namespace(self) -> &'static str {
        match self {
            Self::StableDiffusionWebui => "sd_webui",
            Self::Comfyui => "comfyui",
            Self::Invokeai => "invokeai",
            Self::Fooocus => "fooocus",
            Self::SdTrainer => "sd_trainer",
            Self::QwenTts => "qwen_tts_webui",
            Self::RvcNext => "rvc_next_webui",
            Self::MizuotoTts => "mizuoto_tts_webui",
        }
    }

    pub fn from_api_namespace(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|family| family.api_namespace() == value)
    }

    pub fn api_method(self, operation: &str) -> String {
        format!("{}.{operation}", self.api_namespace())
    }

    /// Name of the core-directory parameter of the family's API methods.
    pub fn api_path_parameter(self) -> &'static str {
        match self {
            Self::StableDiffusionWebui => "sd_webui_path",
            Self::Comfyui => "comfyui_path",
            Self::Invokeai => "invokeai_path",
            Self::Fooocus => "fooocus_path",
            Self::SdTrainer => "sd_trainer_path",
            Self::QwenTts => "qwen_tts_webui_path",
            Self::RvcNext => "rvc_next_webui_path",
            Self::MizuotoTts => "mizuoto_tts_webui_path",
        }
    }

    /// Subcommand of `python -m sd_webui_all_in_one` for the family.
    pub fn cli_command(self) -> &'static str {
        match self {
            Self::StableDiffusionWebui => "sd-webui",
            Self::Comfyui => "comfyui",
            Self::Invokeai => "invokeai",
            Self::Fooocus => "fooocus",
            Self::SdTrainer => "sd-trainer",
            Self::QwenTts => "qwen-tts-webui",
            Self::RvcNext => "rvc-next-webui",
            Self::MizuotoTts => "mizuoto-tts-webui",
        }
    }

    /// Core-directory flag of the family's CLI subcommands. A subcommand that
    /// takes no path is that operation's own fact, decided by its builder.
    pub fn cli_path_flag(self) -> &'static str {
        match self {
            Self::StableDiffusionWebui => "--sd-webui-path",
            Self::Comfyui => "--comfyui-path",
            Self::Invokeai => "--invokeai-path",
            Self::Fooocus => "--fooocus-path",
            Self::SdTrainer => "--sd-trainer-path",
            Self::QwenTts => "--qwen-tts-webui-path",
            Self::RvcNext => "--rvc-next-webui-path",
            Self::MizuotoTts => "--mizuoto-tts-webui-path",
        }
    }
}

#[cfg(test)]
mod tests;
