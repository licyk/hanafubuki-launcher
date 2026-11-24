use crate::internal::hanafubuki_app_update::{UpdateProvider, UpdateSource};

/// Public Hanafubuki release sources shared by the desktop application and the
/// standalone portable launcher.
pub fn hanafubuki_update_sources() -> [UpdateSource; 2] {
    [
        UpdateSource {
            provider: UpdateProvider::Modelscope,
            latest_url: "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/hanafubuki/latest.json".into(),
            catalog_url: "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/hanafubuki/versions.json".into(),
        },
        UpdateSource {
            provider: UpdateProvider::Huggingface,
            latest_url: "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/hanafubuki/latest.json".into(),
            catalog_url: "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/hanafubuki/versions.json".into(),
        },
    ]
}
