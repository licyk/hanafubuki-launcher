use crate::workflow::{LauncherFailure, LauncherFileOperation, LauncherPhase};

use super::message::{Message, PhaseText, mib};

pub(super) fn message(message: Message<'_>) -> String {
    match message {
        Message::RetryingPortableSetup => "正在重试便携环境准备".into(),
        Message::PreparationFailed => "无法准备 Hanafubuki".into(),
        Message::Retry => "重试".into(),
        Message::Exit => "退出".into(),
        Message::DownloadingHanafubuki { version } => {
            format!("正在下载 Hanafubuki {version}")
        }
        Message::UpdatingLauncher { version } => {
            format!("正在更新 Hanafubuki Launcher {version}")
        }
        Message::ScanProgress {
            files,
            directories,
            webuis,
            current_path,
        } => format!(
            "已扫描 {directories} 个目录中的 {files} 个文件；发现 {webuis} 个 WebUI\n当前路径：{current_path}"
        ),
        Message::DownloadProgress {
            completed,
            total: Some(total),
        } => format!("已下载 {:.1} MiB，共 {:.1} MiB", mib(completed), mib(total)),
        Message::DownloadProgress {
            completed,
            total: None,
        } => format!("已接收 {:.1} MiB", mib(completed)),
        Message::FileProgress {
            operation,
            completed,
            total,
            current_path,
        } => file_progress(operation, completed, total, current_path),
    }
}

fn file_progress(
    operation: LauncherFileOperation,
    completed: u64,
    total: Option<u64>,
    current_path: &str,
) -> String {
    let action = match operation {
        LauncherFileOperation::ExtractingLauncher => "正在解压 Launcher 文件",
        LauncherFileOperation::UpdatingLauncher => "正在更新 Launcher 文件",
        LauncherFileOperation::RemovingLauncher => "正在移除 Launcher 文件",
        LauncherFileOperation::ExtractingApplication => "正在解压应用文件",
        LauncherFileOperation::UpdatingApplication => "正在更新应用文件",
        LauncherFileOperation::RemovingApplication => "正在移除应用文件",
    };
    match total {
        Some(total) => format!("{action}：{completed}/{total}\n当前：{current_path}"),
        None => format!("{action}：{completed}\n当前：{current_path}"),
    }
}

pub(super) fn phase(phase: LauncherPhase) -> PhaseText {
    let (title, detail) = match phase {
        #[cfg(windows)]
        LauncherPhase::CheckingLongPathSupport => {
            ("检查 Windows 长路径支持", "正在读取系统文件系统配置")
        }
        LauncherPhase::CheckingSystemProxy => {
            ("检查系统代理", "正在测试系统代理是否可用于网络请求")
        }
        #[cfg(windows)]
        LauncherPhase::CheckingVcRuntime => (
            "检查 Microsoft Visual C++ 运行库",
            "正在读取已安装的 x64 运行库注册信息",
        ),
        #[cfg(windows)]
        LauncherPhase::DownloadingVcRuntime => (
            "下载 Microsoft Visual C++ 运行库",
            "正在从 Microsoft 接收 x64 可再发行组件",
        ),
        #[cfg(windows)]
        LauncherPhase::InstallingVcRuntime => (
            "安装 Microsoft Visual C++ 运行库",
            "正在等待经管理员批准的安装程序",
        ),
        LauncherPhase::CheckingLauncherUpdate => {
            ("检查 Hanafubuki Launcher", "正在连接启动器发布服务")
        }
        LauncherPhase::PreparingLauncherUpdate => (
            "准备 Hanafubuki Launcher 更新",
            "正在创建可信的启动器更新工作目录",
        ),
        LauncherPhase::InstallingLauncherUpdate => (
            "安装 Hanafubuki Launcher 更新",
            "正在将替换操作交给可信更新助手",
        ),
        LauncherPhase::PreparingWorkspace => ("准备 Hanafubuki", "正在创建便携工作目录"),
        LauncherPhase::ScanningWebUis => ("扫描现有 WebUI", "正在检查便携目录结构"),
        LauncherPhase::PreparingPortablePaths => ("准备便携路径", "正在同步 Hanafubuki 存储位置"),
        LauncherPhase::PreparingDiscoveredInstances => ("准备已发现的实例", "正在同步便携实例记录"),
        LauncherPhase::CheckingRelease => ("检查 Hanafubuki 更新", "正在连接发布服务"),
        LauncherPhase::VerifyingInstalledFiles => ("检查 Hanafubuki", "正在校验已安装的便携文件"),
        LauncherPhase::CleaningInstallation => ("清理 Hanafubuki", "正在删除便携应用中的多余文件"),
        LauncherPhase::AnalyzingTrustedCore => ("分析 Hanafubuki", "正在构建可信的便携文件清单"),
        LauncherPhase::UpdatingApplication => ("更新 Hanafubuki", "正在应用便携应用更新"),
        LauncherPhase::RepairingApplication => ("修复 Hanafubuki", "正在修复便携应用文件"),
        LauncherPhase::InstallingApplication => ("安装 Hanafubuki", "正在准备可信的便携应用"),
        LauncherPhase::StartingApplication => ("启动 Hanafubuki", "正在将控制权交给便携应用"),
        LauncherPhase::StartingOffline => (
            "离线启动 Hanafubuki",
            "发布服务不可用，正在使用本地便携应用",
        ),
    };
    PhaseText { title, detail }
}

pub(super) fn failure(failure: &LauncherFailure) -> String {
    match failure {
        LauncherFailure::OfflineRepairRequired => {
            "无法从本地核心启动 Hanafubuki。请连接互联网并重新启动 Launcher，以下载或修复应用。"
                .into()
        }
        LauncherFailure::EventChannelUnavailable => "启动器事件通道不可用。".into(),
        LauncherFailure::WorkerStoppedUnexpectedly => "启动器工作线程意外停止。".into(),
        #[cfg(windows)]
        LauncherFailure::VcRuntimePrerequisite(detail) => {
            format!("无法准备 Microsoft Visual C++ 可再发行组件 (x64)。\n{detail}")
        }
        LauncherFailure::Diagnostic(error) => error.clone(),
    }
}
