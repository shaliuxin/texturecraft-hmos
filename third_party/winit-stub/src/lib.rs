//! OHOS type-only stub for `winit`.
//!
//! The real `winit` crate refuses to compile for `aarch64-unknown-linux-ohos`
//! (`compile_error!("The platform you're compiling for is not supported by winit")`),
//! because it needs a platform backend. HarmonyOS drives the window/event loop and
//! rendering from the native (ArkTS + C++ NAPI) side, so the Rust adapter only needs
//! the *types* that eframe's always-compiled, winit-free code references — no runtime
//! window/event-loop machinery.
//!
//! This stub provides exactly that type surface. Methods that would touch a real
//! backend panic at runtime (they must never be called on OHOS, since the native
//! side owns the loop). It is wired in via the adapter's `[patch.crates-io]` for the
//! OHOS target only; non-OHOS targets keep using the real winit fork.

/// Error types re-exported by eframe's `Error` enum.
pub mod error {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OsError(pub String);

    impl std::fmt::Display for OsError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for OsError {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EventLoopError(pub String);

    impl std::fmt::Display for EventLoopError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for EventLoopError {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NotSupportedError(pub String);

    impl std::fmt::Display for NotSupportedError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for NotSupportedError {}
}

/// Event-loop types referenced by eframe's public API.
pub mod event_loop {
    use std::marker::PhantomData;

    /// Placeholder; the OHOS event loop is owned by the native side.
    #[derive(Clone, Default)]
    pub struct EventLoopBuilder<T> {
        _p: PhantomData<T>,
    }

    #[derive(Default)]
    pub struct EventLoop<T> {
        _p: PhantomData<T>,
    }

    pub struct EventLoopProxy<T> {
        _p: PhantomData<T>,
    }
}

/// Window types referenced by eframe's `CreationContext` / `Frame`.
pub mod window {
    /// Opaque placeholder for a winit window. eframe only ever stores it behind an
    /// `Option<Arc<Window>>` and checks `is_none()`; the native side owns the real
    /// window. No methods are exercised on OHOS.
    #[derive(Clone, Default)]
    pub struct Window;

    #[derive(Clone, Default)]
    pub struct WindowAttributes;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Theme;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct WindowId;
}

/// Platform-specific re-exports eframe references in its public options.
pub mod platform {
    pub mod android {
        pub mod activity {
            /// Opaque placeholder mirroring `android_activity::AndroidApp`. eframe stores
            /// it as `Option<AndroidApp>` in `NativeOptions`; never constructed on OHOS.
            #[derive(Clone)]
            pub struct AndroidApp;
        }
    }

    pub mod startup_notify {
        /// Trait mirror of `winit::platform::startup_notify::WindowExtStartupNotify`.
        /// Only referenced (behind a wayland/x11 cfg that the OHOS build does not take).
        pub trait WindowExtStartupNotify {
            fn request_activation_token(&self) -> Result<(), super::super::error::NotSupportedError> {
                unimplemented!("winit startup_notify is not available on OHOS")
            }
        }
    }
}
