//! The standalone Python builds offered per version, platform, and architecture.

use super::PythonDownload;
use crate::internal::hanafubuki_python_runtime::version::PythonVersion;

pub(super) fn current_platform() -> Result<&'static str, String> {
    match std::env::consts::OS {
        "linux" => Ok("linux"),
        "macos" => Ok("macos"),
        "windows" => Ok("windows"),
        other => Err(format!("unsupported platform: {other}")),
    }
}

pub(super) fn current_arch() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("aarch64"),
        other => Err(format!("unsupported arch: {other}")),
    }
}

pub(super) fn find_download(
    version: PythonVersion,
    platform: &str,
    arch: &str,
) -> Result<PythonDownload, String> {
    PYTHON_DOWNLOADS
        .iter()
        .copied()
        .find(|item| {
            item.version == version.as_str() && item.platform == platform && item.arch == arch
        })
        .ok_or_else(|| {
            format!(
                "no standalone python download for version={version} platform={platform} arch={arch}"
            )
        })
}

/// Mirrors upstream's installer `$python_data` table. Every
/// [`PythonVersion`] must have an entry for each amd64 platform.
///
/// `sha256` is the digest of the archive itself and applies to every URL of
/// the entry. The archives are `install_only.zip` repackages of the
/// python-build-standalone release `20261009` hosted on the
/// `licyk/sd-webui-all-in-one` mirrors; that release's `SHA256SUMS`
/// (<https://github.com/astral-sh/python-build-standalone/releases/download/20261009/SHA256SUMS>)
/// lists only its `.tar.gz` assets and does not cover these files. The digests
/// are the mirrors' own published SHA-256, fetched 2026-11-07 and identical on
/// both: the Hugging Face LFS oid from
/// <https://huggingface.co/api/models/licyk/sd-webui-all-in-one/tree/main/python/{platform}/{arch}>
/// and the ModelScope `Sha256` from
/// <https://modelscope.cn/api/v1/models/licyks/sd-webui-all-in-one/repo/files?Recursive=true&Root=python&Revision=master>.
/// Replacing an archive on the mirror means pinning its new digest here.
pub(super) const PYTHON_DOWNLOADS: &[PythonDownload] = &[
    PythonDownload {
        name: "cpython-3.10.22+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        version: "3.10",
        platform: "linux",
        arch: "aarch64",
        sha256: "bb5206dace6a4f36125271f8145df284e9a215d81c1b95407cb8e4c24b3dbb9e",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/aarch64/cpython-3.10.22+20261009-aarch64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/aarch64/cpython-3.10.22+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.11.17+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        version: "3.11",
        platform: "linux",
        arch: "aarch64",
        sha256: "e19ccd3a821908721fc04c000c55724d5c92ec332186cfc61a42f07318abbfaa",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/aarch64/cpython-3.11.17+20261009-aarch64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/aarch64/cpython-3.11.17+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.12.15+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        version: "3.12",
        platform: "linux",
        arch: "aarch64",
        sha256: "a54bf5670dde0c1b492f8248a8e0fbda35cfc9b02b7ea21e512d07174602c69f",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/aarch64/cpython-3.12.15+20261009-aarch64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/aarch64/cpython-3.12.15+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.13.16+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        version: "3.13",
        platform: "linux",
        arch: "aarch64",
        sha256: "d98150e02604b001cc53665b4eab3f2f9ceaa9aae83c17cdf784e632c8d6ab16",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/aarch64/cpython-3.13.16+20261009-aarch64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/aarch64/cpython-3.13.16+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.14.8+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        version: "3.14",
        platform: "linux",
        arch: "aarch64",
        sha256: "58747f85bfb8a66f2e061b5dab76cec28a2b6f6b3321916490ab21468ad3cb82",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/aarch64/cpython-3.14.8+20261009-aarch64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/aarch64/cpython-3.14.8+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.15.0+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        version: "3.15",
        platform: "linux",
        arch: "aarch64",
        sha256: "a1f08cb8323689f5815540ce46b39e79b09fa4d6b5b472c429c0dc9f885056be",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/aarch64/cpython-3.15.0+20261009-aarch64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/aarch64/cpython-3.15.0+20261009-aarch64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.10.22+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        version: "3.10",
        platform: "linux",
        arch: "amd64",
        sha256: "d73eb591bdab6e27c559999898d9285ccc11c5eca6441ce6872ec0f9ef9024c6",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/amd64/cpython-3.10.22+20261009-x86_64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/amd64/cpython-3.10.22+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.11.17+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        version: "3.11",
        platform: "linux",
        arch: "amd64",
        sha256: "cd2be08e2e86dfa2545be47dfdadef27fc2f7804a57ed04649b05fddac3df433",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/amd64/cpython-3.11.17+20261009-x86_64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/amd64/cpython-3.11.17+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.12.15+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        version: "3.12",
        platform: "linux",
        arch: "amd64",
        sha256: "01649dd3ea321b00f87c7acf120e054e30dfe2dfd0bb4b78ab11ed8b2ae9afb1",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/amd64/cpython-3.12.15+20261009-x86_64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/amd64/cpython-3.12.15+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.13.16+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        version: "3.13",
        platform: "linux",
        arch: "amd64",
        sha256: "04212e17f14430f514b857d77af8838f9593435a02f5ce5f1809b175a370fbab",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/amd64/cpython-3.13.16+20261009-x86_64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/amd64/cpython-3.13.16+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.14.8+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        version: "3.14",
        platform: "linux",
        arch: "amd64",
        sha256: "9caf8fd60c6f0e551a29c2b4470cd9fd3126e0016dd364c145ae58fe427f4a08",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/amd64/cpython-3.14.8+20261009-x86_64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/amd64/cpython-3.14.8+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.15.0+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        version: "3.15",
        platform: "linux",
        arch: "amd64",
        sha256: "c71915e710a5f02c2ec53b7c70f999e715254cd6072d73d7805dc3a944fdf11f",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/linux/amd64/cpython-3.15.0+20261009-x86_64-unknown-linux-gnu-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/linux/amd64/cpython-3.15.0+20261009-x86_64-unknown-linux-gnu-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.10.22+20261009-aarch64-apple-darwin-install_only.zip",
        version: "3.10",
        platform: "macos",
        arch: "aarch64",
        sha256: "2c9c81c1d06f27d869149f9ad665e8c1f8d806c75e427b2c6bbf90c1008e2d9e",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/aarch64/cpython-3.10.22+20261009-aarch64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/aarch64/cpython-3.10.22+20261009-aarch64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.11.17+20261009-aarch64-apple-darwin-install_only.zip",
        version: "3.11",
        platform: "macos",
        arch: "aarch64",
        sha256: "59328299150f6e953ed877214b1e1bfd6ed01de9aa3911be5e5b527ea982270e",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/aarch64/cpython-3.11.17+20261009-aarch64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/aarch64/cpython-3.11.17+20261009-aarch64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.12.15+20261009-aarch64-apple-darwin-install_only.zip",
        version: "3.12",
        platform: "macos",
        arch: "aarch64",
        sha256: "2d5eabdee45ef6b731929dd96bf88f1762d32ef0538e928be327cb7bed2f0ad2",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/aarch64/cpython-3.12.15+20261009-aarch64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/aarch64/cpython-3.12.15+20261009-aarch64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.13.16+20261009-aarch64-apple-darwin-install_only.zip",
        version: "3.13",
        platform: "macos",
        arch: "aarch64",
        sha256: "5342a9c744d814560f0a1db13cff06004568bfc35a2ff1a4b8411afa89b7ee2e",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/aarch64/cpython-3.13.16+20261009-aarch64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/aarch64/cpython-3.13.16+20261009-aarch64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.14.8+20261009-aarch64-apple-darwin-install_only.zip",
        version: "3.14",
        platform: "macos",
        arch: "aarch64",
        sha256: "d083ab8eea0db284444d6b7063c4c29b0adec9b8c0a6dcf880b1d4583e0d2348",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/aarch64/cpython-3.14.8+20261009-aarch64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/aarch64/cpython-3.14.8+20261009-aarch64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.15.0+20261009-aarch64-apple-darwin-install_only.zip",
        version: "3.15",
        platform: "macos",
        arch: "aarch64",
        sha256: "36ac48b5c1f468ee9ee65a5757c5446782fd6bde494038e413b1d47630b7e1b3",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/aarch64/cpython-3.15.0+20261009-aarch64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/aarch64/cpython-3.15.0+20261009-aarch64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.10.22+20261009-x86_64-apple-darwin-install_only.zip",
        version: "3.10",
        platform: "macos",
        arch: "amd64",
        sha256: "d6a6805c5e1a477f48118989783a0473760b452267883dd1c92a565b017cf0b8",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/amd64/cpython-3.10.22+20261009-x86_64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/amd64/cpython-3.10.22+20261009-x86_64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.11.17+20261009-x86_64-apple-darwin-install_only.zip",
        version: "3.11",
        platform: "macos",
        arch: "amd64",
        sha256: "68e08b182ccf30fa4aa79897df813154c03c26ece3644c77b8eebd466f8564c9",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/amd64/cpython-3.11.17+20261009-x86_64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/amd64/cpython-3.11.17+20261009-x86_64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.12.15+20261009-x86_64-apple-darwin-install_only.zip",
        version: "3.12",
        platform: "macos",
        arch: "amd64",
        sha256: "6433ef5744a53d6efea56edad445352959ab21faa8f2e7089a9c6c1ebbdc1347",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/amd64/cpython-3.12.15+20261009-x86_64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/amd64/cpython-3.12.15+20261009-x86_64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.13.16+20261009-x86_64-apple-darwin-install_only.zip",
        version: "3.13",
        platform: "macos",
        arch: "amd64",
        sha256: "473779f4ea7bda39af1606471d1b9db596941cb011d04e6addc8680b183c0e16",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/amd64/cpython-3.13.16+20261009-x86_64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/amd64/cpython-3.13.16+20261009-x86_64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.14.8+20261009-x86_64-apple-darwin-install_only.zip",
        version: "3.14",
        platform: "macos",
        arch: "amd64",
        sha256: "6d6afd532e33b612b06f265005c0768ba2b67408fdab6e88e988e0d798cc0713",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/amd64/cpython-3.14.8+20261009-x86_64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/amd64/cpython-3.14.8+20261009-x86_64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.15.0+20261009-x86_64-apple-darwin-install_only.zip",
        version: "3.15",
        platform: "macos",
        arch: "amd64",
        sha256: "4e66a8dfd5e0b08ac05259f8eb896815a90ae406af2bf5a71042766b6355d9f6",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/macos/amd64/cpython-3.15.0+20261009-x86_64-apple-darwin-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/macos/amd64/cpython-3.15.0+20261009-x86_64-apple-darwin-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.11.17+20261009-aarch64-pc-windows-msvc-install_only.zip",
        version: "3.11",
        platform: "windows",
        arch: "aarch64",
        sha256: "10b50605e4b2358bb62327bc48e9d3c6363eab616c46ee24699999c0d32fb7bc",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/aarch64/cpython-3.11.17+20261009-aarch64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/aarch64/cpython-3.11.17+20261009-aarch64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.12.15+20261009-aarch64-pc-windows-msvc-install_only.zip",
        version: "3.12",
        platform: "windows",
        arch: "aarch64",
        sha256: "537dd3581e2719cdf9e9034113f2df308e22c837c72e5f061a5583beb1977520",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/aarch64/cpython-3.12.15+20261009-aarch64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/aarch64/cpython-3.12.15+20261009-aarch64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.13.16+20261009-aarch64-pc-windows-msvc-install_only.zip",
        version: "3.13",
        platform: "windows",
        arch: "aarch64",
        sha256: "f92d5ffaeaf262ff829c6c385722042e0d2be7685ae531767f546b44b6a0d5f2",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/aarch64/cpython-3.13.16+20261009-aarch64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/aarch64/cpython-3.13.16+20261009-aarch64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.14.8+20261009-aarch64-pc-windows-msvc-install_only.zip",
        version: "3.14",
        platform: "windows",
        arch: "aarch64",
        sha256: "61da82d2212f317db0b9e073fb8cc26dd5b238d24c55da4c79b7cdb94cfc0be5",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/aarch64/cpython-3.14.8+20261009-aarch64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/aarch64/cpython-3.14.8+20261009-aarch64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.15.0+20261009-aarch64-pc-windows-msvc-install_only.zip",
        version: "3.15",
        platform: "windows",
        arch: "aarch64",
        sha256: "b956181493e89f8a35448cceb52e8cd9fd8d750de7c4ff26c99d71e653ba8d02",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/aarch64/cpython-3.15.0+20261009-aarch64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/aarch64/cpython-3.15.0+20261009-aarch64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.10.22+20261009-x86_64-pc-windows-msvc-install_only.zip",
        version: "3.10",
        platform: "windows",
        arch: "amd64",
        sha256: "4cdec16a310d73c8a60f922275cd154996c3b02d024b8f56f804e9d23a152286",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/amd64/cpython-3.10.22+20261009-x86_64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/amd64/cpython-3.10.22+20261009-x86_64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.11.17+20261009-x86_64-pc-windows-msvc-install_only.zip",
        version: "3.11",
        platform: "windows",
        arch: "amd64",
        sha256: "257a169db3f0f73e29542499c6f54ac14a9580ff754bbf8e9b1d81b98ccffe5e",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/amd64/cpython-3.11.17+20261009-x86_64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/amd64/cpython-3.11.17+20261009-x86_64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.12.15+20261009-x86_64-pc-windows-msvc-install_only.zip",
        version: "3.12",
        platform: "windows",
        arch: "amd64",
        sha256: "eff360a5884e826f1038abcb58b4ca99b7e4d32985fe76f6ca160973fe0fc827",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/amd64/cpython-3.12.15+20261009-x86_64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/amd64/cpython-3.12.15+20261009-x86_64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.13.16+20261009-x86_64-pc-windows-msvc-install_only.zip",
        version: "3.13",
        platform: "windows",
        arch: "amd64",
        sha256: "f98d5f36da53d4d6bb6f660b55b3fa597eff670bb8089434b1093997d80043ad",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/amd64/cpython-3.13.16+20261009-x86_64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/amd64/cpython-3.13.16+20261009-x86_64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.14.8+20261009-x86_64-pc-windows-msvc-install_only.zip",
        version: "3.14",
        platform: "windows",
        arch: "amd64",
        sha256: "9c56bb6e03ebcc5652e9bdfb05b131e602eb8a33d95a80d9c96dfd69d2497869",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/amd64/cpython-3.14.8+20261009-x86_64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/amd64/cpython-3.14.8+20261009-x86_64-pc-windows-msvc-install_only.zip",
        ],
    },
    PythonDownload {
        name: "cpython-3.15.0+20261009-x86_64-pc-windows-msvc-install_only.zip",
        version: "3.15",
        platform: "windows",
        arch: "amd64",
        sha256: "e651812ff624c0323731616258ca18dc88e0b5f260efbdd38802125da1ecec10",
        urls: &[
            "https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/python/windows/amd64/cpython-3.15.0+20261009-x86_64-pc-windows-msvc-install_only.zip",
            "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/python/windows/amd64/cpython-3.15.0+20261009-x86_64-pc-windows-msvc-install_only.zip",
        ],
    },
];
