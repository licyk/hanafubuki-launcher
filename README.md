# hanafubuki

一个启动启动器的启动器。

它准备 Hanafubuki 的工作区，更新自己和应用程序，然后启动 Hanafubuki。

## 获取源码

本 crate 发布在 crates.io 上，只作为源码分发渠道，不用于安装。

下载 crate 包：

```sh
# 该端点会 302 跳到 static.crates.io，并把文件命名为 download，
# 所以用 -L 跟随重定向、用 -o 指定输出名。
curl -sSL -o hanafubuki-0.1.0.crate \
  https://crates.io/api/v1/crates/hanafubuki/0.1.0/download
tar xzf hanafubuki-0.1.0.crate
cd hanafubuki-0.1.0
```

也可以把本 crate 加进一个临时项目的依赖，再在 Cargo 的 registry 源码缓存里读取：

```sh
cargo new --lib peek && cd peek
cargo add hanafubuki
cargo fetch
# 源码位于 Cargo 主目录下的 registry/src/<索引>/hanafubuki-0.1.0，
# 主目录默认是 ~/.cargo（Windows 上常是 %USERPROFILE%\.cargo）
```
