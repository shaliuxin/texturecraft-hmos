//! 鸿蒙专属 FFI 边界。TextureCraft 引擎与全部 UI/绘制逻辑保持在安全 Rust 上游（texturecraft-ui-egui /
//! texturecraft-engine）；本 crate 只做 FFI + 渲染 + 事件泵。
//!
//! 与 gridcraft 的关键差异：GridCraft 的 `SheetApp` 已把宿主缝直接暴露为
//! `new(Session, Services)` / `logic(ctx)` / `ui(ui)`，无需 patch 上游。TextureCraft 的
//! `TextureCraftApp` 是 `impl eframe::App`，与 `eframe::Frame` 绑死。已在 ui-egui 加了无 `Frame`
//! 的宿主入口（`new_for_host` / `logic_host` / `ui_host`），本适配层完全**不构造 `eframe::Frame`**
//! （避免 winit 依赖）：桌面/网页走原 `eframe::App` 路径，鸿蒙走宿主入口。
#![allow(clippy::needless_lifetimes)]
mod appearance;
mod bridge;
mod display_color;
mod file_open;
mod form_factor;
mod input;
mod locale;
mod render;

use serde::Deserialize;
use texturecraft_ui_egui::TextureCraftApp;
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

/// 把 `log` crate 的记录转到 hilog：wgpu-hal 的 EGL 诊断（平台选择 / surface 创建 / swap 失败）
/// 默认没有 logger 就会被丢掉，设备上完全看不见。
#[cfg(target_env = "ohos")]
mod glog {
    use std::ffi::{CString, c_char};

    unsafe extern "C" {
        fn tc_hilog(level: i32, message: *const c_char);
    }

    struct Hilog;

    impl log::Log for Hilog {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &log::Record<'_>) {
            let level = match record.level() {
                log::Level::Error => 2,
                log::Level::Warn => 1,
                _ => 0,
            };
            let line = format!("[{}] {}: {}", record.level(), record.target(), record.args());
            if let Ok(text) = CString::new(line) {
                // SAFETY: 有效的 NUL 结尾 C 字符串；tc_hilog 只读该指针。
                unsafe { tc_hilog(level, text.as_ptr()) };
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: Hilog = Hilog;

    pub fn install() {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Config {
    files: String,
    #[serde(default)]
    documents: String,
    #[serde(default = "default_scale")]
    scale: f32,
    #[serde(default)]
    language: String,
    /// 系统 locale（`zh-Hans-CN` 这类 BCP-47 标签），决定「跟随系统」解析到哪种语言。
    #[serde(default, rename = "systemLocale")]
    system_locale: String,
    #[serde(default, rename = "displayP3")]
    display_p3: bool,
    #[serde(default, rename = "systemTheme")]
    system_theme: String,
    #[serde(default)]
    fullscreen: bool,
    #[serde(default, rename = "deviceType")]
    device_type: String,
    #[serde(default, rename = "widthVp")]
    width_vp: f32,
}
fn default_scale() -> f32 {
    1.
}

#[allow(dead_code)]
enum Message {
    Attach(usize, u32, u32),
    Resize(u32, u32),
    Detach,
    Frame,
    Active(bool),
    Input(input::Input),
    Open(String, Vec<u8>),
    Drop(String, Vec<u8>, f32, f32),
    DropFile(String, std::fs::File, f32, f32),
    Close,
    Wake(Duration),
    Command(String, serde_json::Value),
}

static SEND: OnceLock<mpsc::Sender<Message>> = OnceLock::new();
fn send(message: Message) {
    if let Some(tx) = SEND.get() {
        let _ = tx.send(message);
    }
}

/// # Safety
/// `ptr`/`len` 必须在调用期间指向可读内存；调用方保证。
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if len > isize::MAX as usize || (ptr.is_null() && len != 0) {
        return None;
    }
    if len == 0 {
        return Some(&[]);
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

fn copy_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(data.len())
        .map_err(|e| format!("Not enough memory to open file: {e}"))?;
    bytes.extend_from_slice(data);
    Ok(bytes)
}

fn write_temp_file(dir: &str, name: &str, bytes: &[u8]) -> std::io::Result<String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let path = std::path::Path::new(dir).join(format!("{stamp}_{safe}"));
    std::fs::write(&path, bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

// ===================== FFI 导出（C++ 边界调用） =====================

/// # Safety
/// 指针/长度对必须指向可读内存直到本调用返回；notify 回调在进程生命周期内保持有效。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_init(ptr: *const u8, len: usize, notify: bridge::Notify) -> bool {
    let Some(data) = (unsafe { bytes(ptr, len) }) else {
        return false;
    };
    let Ok(config) = serde_json::from_slice::<Config>(data) else {
        return false;
    };
    if !config.scale.is_finite() || config.scale <= 0. || config.scale > 8. {
        return false;
    }
    bridge::init(notify);
    #[cfg(target_env = "ohos")]
    glog::install();
    // 必须在 UI 创建之前：TextureCraft 的「跟随系统」语言经 `i18n::set_language("")` →
    // `sys_locale::get_locale()`（读 LANG 等环境变量）首次求值；先把系统 locale 写进环境变量，
    // 再让 app 读取。
    locale::install(&config.files, &config.system_locale, &config.language);
    if !config.system_theme.is_empty() {
        let dark = config.system_theme.eq_ignore_ascii_case("dark");
        appearance::set_dark(dark);
    }
    let (tx, rx) = mpsc::channel();
    if SEND.set(tx).is_err() {
        return true;
    }
    std::thread::Builder::new()
        .name("texturecraft-native".into())
        .spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker(config, rx)))
                .is_err()
            {
                bridge::notify(0, "error", "Native host initialization failed", &[]);
            }
        })
        .is_ok()
}

/// # Safety
/// 指针/长度对必须可读直到返回。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_input(ptr: *const u8, len: usize) {
    let Some(data) = (unsafe { bytes(ptr, len) }) else { return; };
    let Ok(value) = serde_json::from_slice(data) else { return; };
    send(Message::Input(value));
}

/// # Safety
/// 同 `tc_init`。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_reply(
    id: u64,
    ok: bool,
    text: *const u8,
    text_len: usize,
    data: *const u8,
    len: usize,
) {
    let Some(text) = (unsafe { bytes(text, text_len) }) else {
        bridge::reply(id, Err("Invalid reply text buffer".into()));
        return;
    };
    let Some(data) = (unsafe { bytes(data, len) }) else {
        bridge::reply(id, Err("Invalid reply data buffer".into()));
        return;
    };
    let text = String::from_utf8_lossy(text).into_owned();
    bridge::reply(
        id,
        if ok {
            copy_bytes(data).map(|data| (text, data))
        } else {
            Err(text)
        },
    );
}

/// # Safety
/// 指针/长度对必须可读直到返回。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_open(text: *const u8, n: usize, data: *const u8, len: usize) {
    if let (Some(text), Some(data)) = (unsafe { bytes(text, n) }, unsafe { bytes(data, len) }) {
        match copy_bytes(data) {
            Ok(data) => send(Message::Open(String::from_utf8_lossy(text).into_owned(), data)),
            Err(error) => bridge::notify(0, "error", &error, &[]),
        }
    }
}

/// # Safety
/// 指针/长度对必须可读直到返回。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_drop(
    text: *const u8,
    n: usize,
    data: *const u8,
    len: usize,
    x: f32,
    y: f32,
) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    if let (Some(text), Some(data)) = (unsafe { bytes(text, n) }, unsafe { bytes(data, len) }) {
        send(Message::Drop(
            String::from_utf8_lossy(text).into_owned(),
            data.to_vec(),
            x,
            y,
        ));
    }
}

/// # Safety
/// `text`/`n` 必须可读且 `fd` 打开直到返回。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_drop_file(text: *const u8, n: usize, fd: i32, x: f32, y: f32) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    let Some(text) = (unsafe { bytes(text, n) }) else {
        return;
    };
    match unsafe { file_open::duplicate(fd) } {
        Ok(file) => send(Message::DropFile(
            String::from_utf8_lossy(text).into_owned(),
            file,
            x,
            y,
        )),
        Err(error) => bridge::notify(0, "error", &error, &[]),
    }
}

/// # Safety
/// `ptr`/`len` 为自由文本命令标识。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_command(ptr: *const u8, len: usize) {
    let Some(data) = (unsafe { bytes(ptr, len) }) else { return; };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) else { return; };
    let Some(id) = v.get("id").and_then(|v| v.as_str()) else { return; };
    send(Message::Command(
        id.into(),
        v.get("params").cloned().unwrap_or_default(),
    ));
}

/// # Safety
/// `window` 必须是存活的 OHNativeWindow，且已获取一个原生引用；该引用所有权转移给 worker 及其 wgpu surface。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_attach(window: usize, w: u32, h: u32) {
    if window != 0 {
        send(Message::Attach(window, w, h));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_resize(w: u32, h: u32) {
    send(Message::Resize(w, h));
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_detach() {
    bridge::cancel();
    send(Message::Detach);
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_frame() {
    send(Message::Frame);
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_active(active: bool) {
    send(Message::Active(active));
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_close() {
    send(Message::Close);
}

// ===================== 仅 OHOS 宿主逻辑 =====================

#[cfg(target_env = "ohos")]
unsafe extern "C" {
    fn tc_request_vsync();
}

/// 内存版 `eframe::Storage`：首版把 UI 状态 / 模板库的持久化留在内存（鸿蒙沙箱外的文件由 ArkTS
/// 经 picker + fileIo 写盘，故 `save` best-effort、不落盘，不阻断启动）。
#[cfg(target_env = "ohos")]
#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);

#[cfg(target_env = "ohos")]
impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

/// 把 BCP-47 系统语言标签映射到 texturecraft 的界面语言码；映射不到则用系统默认（跟随系统）。
#[cfg(target_env = "ohos")]
fn language_code(tag: &str) -> Option<&'static str> {
    let primary = tag.trim().split(['-', '_']).next().unwrap_or(tag.trim());
    if primary.is_empty() {
        return None;
    }
    Some(match primary {
        "en" => "en",
        "fr" => "fr",
        "de" => "de",
        "es" => "es",
        "pt" => "pt-BR",
        "it" => "it",
        "ja" => "ja",
        "ko" => "ko",
        "ar" => "ar",
        // 简体中文 / 繁体中文 鸿蒙统一用 SC 面，界面走 zh-Hans。
        "zh" => "zh-Hans",
        _ => return None,
    })
}

#[cfg(target_env = "ohos")]
fn worker(config: Config, rx: mpsc::Receiver<Message>) {
    bridge::notify(0, "stage", "Rust worker entered", &[]);
    let ctx = egui::Context::default();
    let form = form_factor::Form::new(&config.device_type, config.width_vp);
    bridge::notify(
        0,
        "diagnostic",
        &format!(
            "form factor {:?}, {:.0} vp, {:?}",
            form.device_type, form.width_vp, form.width_class
        ),
        &[],
    );

    // TextureCraft 的 session：空工程态（无项目打开）。新工程由 UI 或 `file.new` 命令创建。
    let session = match texturecraft_engine::Session::closed() {
        Ok(session) => session,
        Err(e) => {
            bridge::notify(0, "error", &format!("session: {e}"), &[]);
            return;
        }
    };

    // 保存后端：内存 KV（首版）。`new_for_host` 用它加载已存 UI 状态（此处为空）。
    let mut storage = MemoryStorage::default();

    // 起始 render_state 为 None：attach 时再绑定 GLES 并回填。3D 视口在 GLES 上由上游
    // `device_supports_tracing` 优雅降级——v1 不要求 3D 跑通，只要求 2D 主 UI 渲染。
    let mut app = TextureCraftApp::new_for_host(&ctx, None, Some(&storage), session);

    // 界面语言：默认「跟随系统」（i18n::SYSTEM_LANGUAGE == ""，经 locale::install 写好的 LANG 解析）。
    // 优先用 config.language（ArkTS 的 getSystemLanguage），映射不到则用 config.system_locale
    // （getSystemLocale，如 "zh-Hans-CN"），再退到 sys_locale 读环境变量。
    let mut lang_code: Option<&'static str> = language_code(&config.language);
    if lang_code.is_none() && !config.system_locale.is_empty() {
        lang_code = language_code(&config.system_locale);
    }
    bridge::notify(
        0,
        "diagnostic",
        &format!(
            "lang: systemLang={} systemLocale={} mapped={:?} prefs={}",
            config.language, config.system_locale, lang_code, app.ui.prefs.language
        ),
        &[],
    );
    if let Some(code) = lang_code {
        app.ui.prefs.language = code.to_owned();
        texturecraft_ui_egui::i18n::set_language(&app.ui.prefs.language);
    }
    bridge::notify(
        0,
        "diagnostic",
        &format!("interface language: {}", texturecraft_ui_egui::i18n::current().code),
        &[],
    );

    // 鸿蒙 CJK 字体注入（best-effort）：texturecraft 的 `Script::system_files()` 只扫固定绝对路径，
    // 沙箱写不进，故把系统字体字节经 egui 0.37 的 `Context::add_font` 以 `FontPriority::Lowest` 追加到
    // 所有字体族（只在既有字体缺字形时兜底，不改上游字体定义与主题）。
    //
    // 记录当前已注入的语言：上游 `fonts::update` 在语言切换时用 `set_fonts` 整体替换字体定义，会盖掉
    // 此注入；帧循环在检测到 `prefs.language` 变化后调用 `inject_cjk_fallback` 重新注入（见 Message::Frame）。
    let mut injected_language = app.ui.prefs.language.clone();
    inject_cjk_fallback(&ctx, &injected_language);

    bridge::notify(0, "stage", "editor ready", &[]);

    // 起始逻辑帧（无用户输入）：让 app 在宿主登记后先跑一次 logic。
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.logic_host(ctx));

    let mut render = match render::Render::new(config.display_p3) {
        Ok(render) => render,
        Err(error) => {
            bridge::notify(0, "error", &error, &[]);
            return;
        }
    };
    let mut active = true;
    let mut visible = false;
    let mut deadline: Option<Instant> = None;
    let started = Instant::now();
    let mut events: Vec<egui::Event> = Vec::new();
    let mut input_state = input::State::default();
    let mut close = false;
    // 软键盘是否已由宿主弹起：避免每帧重复通知，仅在 egui 文本焦点变化时切换。
    let mut ime_shown = false;

    ctx.set_request_repaint_callback(|info| send(Message::Wake(info.delay)));
    bridge::notify(0, "ready", "Native host initialized", &[]);

    loop {
        let wait = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        let message = match wait {
            Some(wait) => match rx.recv_timeout(wait) {
                Ok(m) => m,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    deadline = None;
                    if active && visible && render.attached() {
                        unsafe { tc_request_vsync() };
                    }
                    continue;
                }
                Err(_) => break,
            },
            None => match rx.recv() {
                Ok(m) => m,
                Err(_) => break,
            },
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<(), String> {
                match message {
                    Message::Attach(ptr, w, h) => {
                        visible = w > 0 && h > 0;
                        render.bind(render::Window(ptr), w, h)?;
                        // 绑定 GLES 后把 RenderState 回填给 app，并 best-effort 初始化 3D 的 GpuLink。
                        // gpu::init 在 GLES 上可能不可用：包裹 catch_unwind，失败只跳过 3D、不阻断启动。
                        app.render_state = render.egui_render_state();
                        if let Some(rs) = app.render_state.as_ref() {
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                texturecraft_ui_egui::gpu::init(rs, &ctx)
                            })) {
                                Ok(link) => app.gpu = Some(link),
                                Err(_) => bridge::notify(0, "diagnostic", "3D GpuLink init skipped on GLES", &[]),
                            }
                        }
                        bridge::resume();
                        deadline = Some(Instant::now());
                        // 重连后重置软键盘状态，下一帧按实际焦点重新评估显隐。
                        ime_shown = false;
                        bridge::notify(0, "ready", "Native egui / wgpu GLES", &[]);
                    }
                    Message::Resize(w, h) => {
                        visible = w > 0 && h > 0;
                        render.resize(w, h)?;
                        deadline = Some(Instant::now());
                    }
                    Message::Detach => {
                        visible = false;
                        deadline = None;
                        render.detach();
                        input_state.cancel(&mut events);
                        // 表面销毁时收起软键盘并重置状态，避免重连后状态错乱。
                        ime_shown = false;
                        bridge::notify(0, "ime", "hide", &[]);
                    }
                    Message::Active(value) => {
                        active = value;
                        if active {
                            bridge::resume();
                            deadline = Some(Instant::now());
                        } else {
                            deadline = None;
                            input_state.cancel(&mut events);
                        }
                    }
                    Message::Input(value) => {
                        if value.kind == "system_theme" {
                            let dark = value.text.eq_ignore_ascii_case("dark");
                            appearance::set_dark(dark);
                            deadline = Some(Instant::now());
                            return Ok(());
                        }
                        input_state.push(value, config.scale, &mut events);
                        deadline = Some(Instant::now());
                    }
                    Message::Open(name, data) => {
                        if let Ok(path) = write_temp_file(&config.files, &name, &data) {
                            open_into_app(&mut app, &path);
                        }
                        deadline = Some(Instant::now());
                    }
                    Message::Drop(name, data, _x, _y) => {
                        if let Ok(path) = write_temp_file(&config.files, &name, &data) {
                            open_into_app(&mut app, &path);
                        }
                        deadline = Some(Instant::now());
                    }
                    Message::DropFile(name, file, _x, _y) => {
                        match file_open::read(file) {
                            Ok(data) => {
                                if let Ok(path) = write_temp_file(&config.files, &name, &data) {
                                    open_into_app(&mut app, &path);
                                }
                            }
                            Err(e) => bridge::notify(0, "error", &e, &[]),
                        }
                        deadline = Some(Instant::now());
                    }
                    Message::Close => {
                        bridge::resume();
                        close = true;
                        deadline = Some(Instant::now());
                    }
                    Message::Wake(delay) => {
                        if active && visible {
                            if let Some(next) = Instant::now().checked_add(delay) {
                                deadline = Some(deadline.map_or(next, |d| d.min(next)));
                            }
                        }
                    }
                    Message::Command(id, params) => {
                        let result = app.run(&id, params);
                        bridge::notify(
                            0,
                            "command",
                            &format!("{id}: {result:?}"),
                            &[],
                        );
                        deadline = Some(Instant::now());
                    }
                    Message::Frame => {
                        if !active || !visible {
                            return Ok(());
                        }
                        if !render.attached() {
                            return Ok(());
                        }
                        let size = egui::vec2(
                            render.width as f32 / config.scale,
                            render.height as f32 / config.scale,
                        );
                        let mut raw = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            time: Some(started.elapsed().as_secs_f64()),
                            events: std::mem::take(&mut events),
                            system_theme: appearance::current(),
                            ..Default::default()
                        };
                        let viewport = raw.viewports.entry(egui::ViewportId::ROOT).or_default();
                        viewport.native_pixels_per_point = Some(config.scale);
                        viewport.inner_rect = raw.screen_rect;
                        viewport.focused = Some(true);
                        let validating_close = close;
                        if close {
                            viewport.events.push(egui::ViewportEvent::Close);
                            close = false;
                        }
                        // TextureCraft 的宿主缝：logic_host / ui_host（不构造 eframe::Frame）。
                        let output = editor_frame(&ctx, &mut app, raw);
                        // 语言切换时上游 `fonts::update` 用 `set_fonts` 整体替换字体定义，会盖掉
                        // 启动时的 CJK 注入。检测到 `prefs.language` 变化后在 paint 前重新注入，
                        // 使本帧就恢复中文兜底；仅在变化那一刻重读字节，稳态不重复。
                        if app.ui.prefs.language != injected_language {
                            injected_language = app.ui.prefs.language.clone();
                            inject_cjk_fallback(&ctx, &injected_language);
                        }
                        for command in &output.platform_output.commands {
                            if let egui::OutputCommand::OpenUrl(url) = command {
                                bridge::notify(0, "url", &url.url, &[]);
                            }
                        }
                        // 软键盘随文本焦点自动弹起/收起：egui 在有文本字段获得焦点时把
                        // `platform_output.ime` 置为 Some，宿主据此显隐系统输入法键盘。
                        let ime_wanted = output.platform_output.ime.is_some();
                        if ime_wanted != ime_shown {
                            ime_shown = ime_wanted;
                            bridge::notify(0, "ime", if ime_wanted { "show" } else { "hide" }, &[]);
                        }
                        let mut commands = Vec::new();
                        let mut repaint_delay: Option<Duration> = None;
                        if let Some(v) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                            commands.extend(v.commands.clone());
                            repaint_delay = Some(v.repaint_delay);
                        }
                        match close_action(validating_close, &commands) {
                            CloseAction::Confirm => {
                                // 确认关窗：把 UI 状态 best-effort 存进内存存储，再通知宿主销毁。
                                // `save` 是 `eframe::App` 的 trait 方法，宿主侧用全限定 UFCS 调用，
                                // 避免在适配层引入整份 `eframe::App` trait 作用域。
                                eframe::App::save(&mut app, &mut storage);
                                bridge::notify(0, "close", "", &[]);
                            }
                            CloseAction::Request => {
                                close = true;
                                deadline = Some(Instant::now());
                            }
                            CloseAction::Cancel | CloseAction::None => {}
                        }
                        let painted = render.paint(&ctx, output)?;
                        if first_frame_marker(painted) {
                            bridge::notify(0, "first_frame", "Native UI presented", &[]);
                        }
                        // 按 egui 的 `repaint_delay` 排下一帧：视口在动时它是 0（连续重绘），
                        // 静止时是一个很大的值或 MAX（等输入/vsync 唤醒），避免空转耗电。
                        if painted && active && visible && render.attached() {
                            deadline = match repaint_delay {
                                Some(delay) if delay.is_zero() => Some(Instant::now()),
                                Some(delay) if delay < Duration::from_secs(3600) => {
                                    Some(Instant::now() + delay)
                                }
                                _ => None,
                            };
                        }
                    }
                }
                Ok(())
            },
        ));
        let _ = result;
    }
}

#[cfg(target_env = "ohos")]
#[allow(dead_code)]
fn first_frame_marker(painted: bool) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FIRST: AtomicBool = AtomicBool::new(false);
    if painted && !FIRST.swap(true, Ordering::AcqRel) {
        true
    } else {
        false
    }
}

#[cfg(target_env = "ohos")]
fn editor_frame(
    ctx: &egui::Context,
    app: &mut TextureCraftApp,
    raw: egui::RawInput,
) -> egui::FullOutput {
    // 与上游宿主一致：每个 pass 的首拍都推进一次 `logic_host`（内含逐帧 tick），随后把 `ui_host`
    // 画进 CentralPanel。只跑首帧会让界面停摆。
    ctx.run_ui(raw, |ui| {
        if ui.ctx().current_pass_index() == 0 {
            app.logic_host(ui.ctx());
        }
        app.ui_host(ui);
    })
}

/// 把鸿蒙系统 CJK 字体以 `FontPriority::Lowest` 追加到所有字体族（best-effort）。
///
/// 上游 `UiFonts::update()` 在语言切换时用 `ctx.set_fonts(defs)` **整体替换**字体定义，会把
/// 此处的 `add_font` 注入（名为 `hmos-cjk-han`）一并清掉。宿主在每帧检测到 `prefs.language`
/// 变化后重新注入，故此处按当前语言挑面并重读字节——语言变化罕见，~20MB 读盘只发生在切换那一刻，
/// 而非每帧。稳态下语言不变、`fonts::update` 走早退分支，注入不被清，无需重复注入。
#[cfg(target_env = "ohos")]
fn inject_cjk_fallback(ctx: &egui::Context, language: &str) {
    let Some(bytes) = locale::cjk_font_bytes(language) else {
        return;
    };
    let families: Vec<egui::FontFamily> = texturecraft_ui_egui::theme::font_definitions()
        .families
        .keys()
        .cloned()
        .collect();
    let count = families.len();
    ctx.add_font(egui::FontInsert {
        name: "hmos-cjk-han".to_owned(),
        data: egui::FontData::from_owned(bytes),
        families: families
            .into_iter()
            .map(|family| egui::InsertFontFamily {
                family,
                priority: egui::FontPriority::Lowest,
            })
            .collect(),
    });
    log::info!("[tc] CJK fallback injected into {count} families for language {language:?}");
}

#[cfg(target_env = "ohos")]
#[derive(Debug, PartialEq, Eq)]
// `None` 为兜底占位（首版 `close_action` 恒返回 Request/Confirm/Cancel），标 `dead_code` 保留语义。
#[allow(dead_code)]
enum CloseAction {
    None,
    Request,
    Cancel,
    Confirm,
}

/// 关窗决策。TextureCraft 没有 gridcraft 的 `allow_close` 标志：未保存改动由上游
/// `windows::close_request` 发 `CancelClose`（并弹出「未保存」对话框）；无改动则确认关。
/// 关窗是**用户/宿主发起**的：只有视口真的带 `Close` 命令（用户按 X）才 Request，
/// 随后 `validating` 帧 Confirm。空闲帧无 close 命令必须返回 `None`——否则每帧都请求关窗，
/// 应用会在第 2 帧自我关闭转后台（真机 2026-11-25 实测：frame=1 Request→frame=2 Confirm→转后台）。
#[cfg(target_env = "ohos")]
fn close_action(validating: bool, commands: &[egui::ViewportCommand]) -> CloseAction {
    if commands.contains(&egui::ViewportCommand::CancelClose) {
        CloseAction::Cancel
    } else if validating {
        CloseAction::Confirm
    } else if commands.contains(&egui::ViewportCommand::Close) {
        CloseAction::Request
    } else {
        CloseAction::None
    }
}

/// 把沙箱文件路径交给上游 `file.open` 命令（在 activity 文档上打开，未改动则替换，否则新开）。
/// 内部经 `app.run` 报错误为 toast，不返回 Result。
#[cfg(target_env = "ohos")]
fn open_into_app(app: &mut TextureCraftApp, path: &str) {
    // texturecraft 的 `file.open` 接 `{path}`；`.tcraft` 工程或网格文件。
    let _ = app.run("file.open", serde_json::json!({ "path": path }));
    log::info!("[tc] open_path: {path}");
}

#[cfg(not(target_env = "ohos"))]
fn worker(_config: Config, _rx: mpsc::Receiver<Message>) {
    bridge::notify(0, "error", "Native host requires an OHOS target", &[]);
}
