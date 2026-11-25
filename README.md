# TextureCraft for HarmonyOS（鸿蒙）

把 [TextureCraft](https://github.com/storytold/texturecraft) —— 一个纯 Rust 的 **3D PBR 纹理绘制工具**
（egui 0.37 fork + egui-wgpu + wgpu 30 + winit fork + craft-libs）——移植到 **HarmonyOS**，
产出可在真机上安装运行的 DevEco Studio 工程与 `.hap`。

- **移植对象**：`storytold/texturecraft` v0.6.1（Rust 1.95+）。
- **移植方法**：以姊妹工程 `gridcraft-hmos` 为主模板，沿用「ArkTS + C++ N-API + Rust 适配层」
  三层架构与双 ABI 产物。
- **包名（铁律，独立）**：bundleName `io.github.shaliuxin.texturecraft.hmos`、versionName `0.6.1`、
  versionCode `601`、显示名 `TextureCraft`、文件类型 `.gltf`/`.glb`/`.tcraft`。

> ⚠ 与 gridcraft 的**本质差异**：TextureCraft 的 UI 入口 `TextureCraftApp` 是 `impl eframe::App`，
> 其 `logic`/`ui` 绑死 `eframe::Frame`，而 eframe 强依赖 winit、winit **不支持 OHOS**。
> 因此本项目与之前 10+ 个「纯 egui 宿主缝、不 patch 上游」的 craft 项目不同——
> **本移植必须 patch 上游**（eframe 门控 + ui-egui 宿主缝），详见下。

## 架构

```
ArkTS 外壳 (UIAbility + XComponent)
   │  libtexturecraft.so（N-API 模块，模块名 texturecraft）
C++ N-API 桥  (entry/src/main/cpp/bridge.cpp)   窗口/输入/色彩空间/IME/hilog
   │  FFI（C ABI，前缀 tc_）
Rust 适配层  (native/rust/，staticlib libtexturecraft_hmos.a)   事件泵 + egui-wgpu GLES 渲染
   │  path 依赖 + 最小宿主缝补丁
上游安全 Rust (texturecraft/crates/ui-egui + engine)   文档/3D 逻辑零安全改动
```

## 关键改造

1. **eframe / egui-winit OHOS 门控（stub 方案）**：`#[cfg(target_env="ohos")]` 门控掉 eframe 的
   `mod native`/`run_native`/`create_native` 等（见 `third_party/eframe`），`[patch.crates-io]` 把
   `winit`→`third_party/winit-stub`、`egui-winit`→`third_party/egui-winit-stub`、
   `wgpu-hal`→`third_party/wgpu-hal-30.0.1`（鸿蒙 GLES uniform-block 修补）、
   `egui-phosphor`→`texturecraft/vendor/egui-phosphor`。
   **eframe 补丁落在 cargo git 缓存，由 `scripts/restore_eframe.sh` 从 `third_party/eframe` 固化回缓存**
   （构建前自动调用，防清缓存丢失）。
2. **上游宿主缝补丁**（`texturecraft/crates/ui-egui/src/lib.rs`，+86/-27）：抽 `new_common`/
   `logic_impl`/`ui_body`，新增 OHOS 宿主入口 `new_for_host`/`logic_host`/`ui_host`
   （`#[cfg(target_env="ohos")]`，不构造 `eframe::Frame`）；桌面/网页走原 `eframe::App` 路径不变。
3. **3D 视口 GLES 优雅降级**：烘焙/路径追踪用 compute 管线，GLES 不支持，`gpu::init` 包
   `catch_unwind` 做 best-effort——3D 失败不阻塞 2D 主 UI 渲染。

## 构建

```bash
# 1) 编译双 ABI 静态库（脚本内先跑 restore_eframe.sh 固化 eframe 补丁）
bash scripts/build_native.sh
# 2) DevEco 用 io.github.shaliuxin.texturecraft.hmos 自动签名，回写 signing/build-profile.local.json5
# 3) 打包
node <DevEco>/tools/hvigor/bin/hvigorw.js assembleHap --mode module -p product=default -p buildMode=debug --no-daemon
# 4) 校验产物（含 libtexturecraft.so、双 ABI ELF、无 web 残留、bundle 正确）
python scripts/verify_native.py entry/build/default/outputs/default/entry-default-signed.hap
# 5) 真机
bash scripts/run_on_device.sh
```

交叉编译 env（Windows NDK）：clang `C:/ohosnative/llvm/bin/clang.exe` + sysroot `C:/ohosnative/sysroot`；
需 `CC_<triple>`/`CFLAGS=--target=<triple>-linux-ohos --sysroot=C:/ohosnative/sysroot -D__MUSL__`/
`AR_<triple>`/`CARGO_TARGET_<triple>_LINKER`。完整说明见 [`移植报告.md`](./移植报告.md)。

## 当前状态

- ✅ `cargo check --target aarch64-unknown-linux-ohos` 通过（EXIT=0，warning-clean）；
  host 目标 `cargo check -p texturecraft-ui-egui` 通过（宿主补丁桌面安全）。
- ⏳ HAP 打包 + 真机：本机无鸿蒙设备，未验证。
- ⏳ 待办：CJK 字体每帧覆盖确认、关窗未保存标志、N-API request/reply 桥（v2）、桌面整包 CI。

详见 [`移植报告.md`](./移植报告.md) §5。
