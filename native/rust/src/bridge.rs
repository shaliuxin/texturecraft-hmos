//! 鸿蒙 N-API 桥：Rust 侧只通过一个通知回调上报状态、通过请求/应答通道向 ArkTS 要文件/服务。
//! 平台只等待 Rust worker 线程，绝不阻塞 ArkTS 主线程。
use std::collections::HashMap;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

/// ArkTS 侧的通知回调：`(id, kind, text, data)`。C++ 边界会在返回前拷贝两块借用的切片。
pub type Notify = extern "C" fn(u64, *const u8, usize, *const u8, usize);
static NOTIFY: OnceLock<Notify> = OnceLock::new();
// 请求/应答通道（v1 暂未接线：开文件走 `app.run("file.open")`，文件由 ArkTS 直接传入）。
// 保留 `request`/`NEXT` 作为 v2 的「向 ArkTS 拉文件/服务」通路，故标 `dead_code`。
#[allow(dead_code)]
static NEXT: AtomicU64 = AtomicU64::new(1);
static CANCELLED: AtomicBool = AtomicBool::new(false);
static PENDING: OnceLock<Mutex<HashMap<u64, mpsc::Sender<Reply>>>> = OnceLock::new();

/// 一次请求的结果：`(文件名, 字节)` 或错误串。
pub type Reply = Result<(String, Vec<u8>), String>;

fn pending() -> &'static Mutex<HashMap<u64, mpsc::Sender<Reply>>> {
    PENDING.get_or_init(Default::default)
}

pub fn init(notify: Notify) {
    let _ = NOTIFY.set(notify);
}

/// 向 ArkTS 上报一条状态/事件。kind 形如 "stage"/"error"/"diagnostic"/"ready" 等。
pub fn notify(id: u64, kind: &str, text: &str, data: &[u8]) {
    if let Some(f) = NOTIFY.get() {
        let message = serde_json::json!({ "kind": kind, "text": text }).to_string();
        // C++ 回调会在返回前拷贝这两块借用的切片。
        f(
            id,
            message.as_ptr(),
            message.len(),
            data.as_ptr(),
            data.len(),
        );
    }
}

/// 同步请求：登记应答通道 → 通知平台 → 阻塞等待 ArkTS 回包。返回文件名与字节。
/// 文件对话框 override 经此把请求交给 ArkTS 的系统 picker（见 `file_dialog_answer`）。
pub fn request(kind: &str, text: &str, data: &[u8]) -> Reply {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    {
        let mut map = pending().lock().unwrap_or_else(|p| p.into_inner());
        if CANCELLED.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        map.insert(id, tx);
    }
    notify(id, kind, text, data);
    rx.recv()
        .unwrap_or_else(|_| Err("platform service disconnected".into()))
}

/// 把 craft-dialogs 的文件请求应答为 `Answer`，接进 `set_override`。仅 `Kind::Open` 经平台 picker
/// 取 1 个文件（ArkTS 把选中文件复制进沙箱 `.inbox` 后回传路径，then 闭包再用 std::fs 读该路径）；
/// `Save`/`Folder` 首版未接线其字节/目录通路，安全降级为 `Cancelled`（then 收空路径，不崩）。
///
/// 本函数在 `drain_override` 内的 override 闭包里被调，会阻塞等待 ArkTS 回包（与原生模态对话框同）。
#[cfg(target_env = "ohos")]
pub fn file_dialog_answer(
    req: &texturecraft_ui_egui::file_dialogs::Request,
) -> texturecraft_ui_egui::file_dialogs::Answer {
    use texturecraft_ui_egui::file_dialogs::{Answer, Kind, Request};
    let dialog = match req {
        Request::File(dialog) => dialog,
        Request::Message(_) => return Answer::Cancelled,
    };
    if !matches!(dialog.kind, Kind::Open { .. }) {
        log::info!("[tc] file dialog: non-open kind (Save/Folder) not wired in v1, answer Cancelled");
        return Answer::Cancelled;
    }
    let title = dialog.title.clone().unwrap_or_default();
    log::info!("[tc] file dialog open requested (title={title:?}); awaiting ArkTS picker reply");
    match request("open", &title, &[]) {
        Ok((path, _bytes)) => {
            log::info!("[tc] file dialog open replied: path={path}");
            Answer::Paths(vec![std::path::PathBuf::from(path)])
        }
        Err(why) => {
            log::info!("[tc] file dialog open cancelled/failed: {why}");
            Answer::Cancelled
        }
    }
}

/// 一次性占位窗口句柄：鸿蒙无原生窗口，但 `FileDialogs::drain_override` 需要一个
/// `HasWindowHandle + HasDisplayHandle` 的类型来满足 craft-dialogs `show_pending` 的泛型约束。
/// override 路径从不读取该 parent（仅原生分支才用），故用 dangling 指针造假，绝不解引用。
#[cfg(target_env = "ohos")]
pub struct DialogWindow;

#[cfg(target_env = "ohos")]
impl raw_window_handle::HasWindowHandle for DialogWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // SAFETY: 该句柄永不被读取（override 忽略 parent），dangling 指针仅作类型占位。
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::OhosNdk(
                raw_window_handle::OhosNdkWindowHandle::new(core::ptr::NonNull::<core::ffi::c_void>::dangling()),
            ))
        })
    }
}

#[cfg(target_env = "ohos")]
impl raw_window_handle::HasDisplayHandle for DialogWindow {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(raw_window_handle::DisplayHandle::ohos())
    }
}

/// ArkTS 回包：按 id 找到等待者并投递。
pub fn reply(id: u64, result: Reply) {
    if let Some(tx) = pending()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&id)
    {
        let _ = tx.send(result);
    }
}

/// 生命周期中止时唤醒所有等待者，避免 worker 永久卡在已关闭的系统选择器上。
pub fn cancel() {
    let mut map = pending().lock().unwrap_or_else(|p| p.into_inner());
    CANCELLED.store(true, Ordering::Release);
    for (_, tx) in map.drain() {
        let _ = tx.send(Err("cancelled".into()));
    }
}

pub fn resume() {
    CANCELLED.store(false, Ordering::Release);
}
