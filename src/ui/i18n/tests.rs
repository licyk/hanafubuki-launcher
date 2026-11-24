use crate::workflow::{LauncherFailure, LauncherFileOperation, LauncherPhase};

use super::{I18n, Locale, Message};

#[test]
fn detects_supported_chinese_locale_shapes() {
    for locale in ["zh", "zh-CN", "zh-Hans-CN", "zh_CN.UTF-8"] {
        assert_eq!(Locale::detect(Some(locale)), Locale::SimplifiedChinese);
    }
}

#[test]
fn defaults_unknown_or_missing_locale_to_english() {
    for locale in [None, Some("en-US"), Some("ja-JP"), Some("")] {
        assert_eq!(Locale::detect(locale), Locale::English);
    }
}

#[test]
fn localizes_every_kind_of_dynamic_interface_message() {
    let chinese = I18n::new(Locale::SimplifiedChinese);
    assert_eq!(
        chinese.t(Message::DownloadingHanafubuki { version: "1.2.3" }),
        "正在下载 Hanafubuki 1.2.3"
    );
    assert_eq!(
        chinese.t(Message::ScanProgress {
            files: 12,
            directories: 3,
            webuis: 2,
            current_path: "models/core",
        }),
        "已扫描 3 个目录中的 12 个文件；发现 2 个 WebUI\n当前路径：models/core"
    );
    let mib = 1024 * 1024;
    assert_eq!(
        chinese.t(Message::DownloadProgress {
            completed: mib,
            total: Some(2 * mib),
        }),
        "已下载 1.0 MiB，共 2.0 MiB"
    );
    assert_eq!(
        I18n::new(Locale::English).t(Message::DownloadProgress {
            completed: mib,
            total: None,
        }),
        "1.0 MiB received"
    );
    assert_eq!(
        chinese.t(Message::FileProgress {
            operation: LauncherFileOperation::UpdatingApplication,
            completed: 3,
            total: Some(10),
            current_path: "bin/hanafubuki",
        }),
        "正在更新应用文件：3/10\n当前：bin/hanafubuki"
    );
}

#[test]
fn phase_copy_is_selected_by_semantic_phase() {
    let phase = I18n::new(Locale::SimplifiedChinese).phase(LauncherPhase::CheckingSystemProxy);
    assert_eq!(phase.title, "检查系统代理");
    assert_eq!(phase.detail, "正在测试系统代理是否可用于网络请求");

    let phase = I18n::new(Locale::SimplifiedChinese).phase(LauncherPhase::StartingOffline);
    assert_eq!(phase.title, "离线启动 Hanafubuki");
    assert_eq!(phase.detail, "发布服务不可用，正在使用本地便携应用");

    #[cfg(windows)]
    {
        let phase = I18n::new(Locale::SimplifiedChinese).phase(LauncherPhase::DownloadingVcRuntime);
        assert_eq!(phase.title, "下载 Microsoft Visual C++ 运行库");
        assert_eq!(phase.detail, "正在从 Microsoft 接收 x64 可再发行组件");
    }
}

#[test]
fn known_failures_are_localized_and_diagnostics_are_preserved() {
    let chinese = I18n::new(Locale::SimplifiedChinese);
    assert_eq!(
        chinese.failure(&LauncherFailure::WorkerStoppedUnexpectedly),
        "启动器工作线程意外停止。"
    );
    assert_eq!(
        chinese.failure(&LauncherFailure::Diagnostic(
            "HTTP 500: /release/path".into()
        )),
        "HTTP 500: /release/path"
    );
    #[cfg(windows)]
    assert_eq!(
        chinese.failure(&LauncherFailure::VcRuntimePrerequisite(
            "registry unavailable".into()
        )),
        "无法准备 Microsoft Visual C++ 可再发行组件 (x64)。\nregistry unavailable"
    );
}
