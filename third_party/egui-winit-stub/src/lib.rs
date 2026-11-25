//! OHOS type-only stub for `egui-winit`.
//!
//! The real `egui-winit` translates winit events into egui input, manages the
//! OS clipboard/IME, and processes viewport commands — all of which require a
//! winit backend that does not exist for `aarch64-unknown-linux-ohos`. On OHOS the
//! native side owns the event loop and rendering (via egui-wgpu), and eframe's
//! always-compiled code references no `egui_winit` items once eframe's `native`
//! module is gated off. This crate therefore compiles to nothing for the OHOS
//! target and is wired in via the adapter's `[patch.crates-io]` for OHOS only.
