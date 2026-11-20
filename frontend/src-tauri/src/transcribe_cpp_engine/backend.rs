//! Compute-backend selection for the transcribe.cpp engine.
//!
//! The library ships its compute backends as loadable modules next to
//! `libtranscribe` (ggml dynamic-backend build), registered by
//! `transcribe_init_backends_ex` under an allowed-backends mask that is
//! fixed at the FIRST registration. This module maps the app-facing
//! [`ComputeMode`] onto that mask, the `transcribe_backend_request` for
//! model load, and a device picked from the registry.
//!
//! The pinned linux/windows lanes ship `cpu` + `vulkan` modules and the
//! macOS lane ships `metal` (native-facts.md §1); the contract's
//! `backends` list records what the loaded lane actually contains.
//!
//! Everything here is expressed against the [`DeviceQuery`] seam, so the
//! mapping is unit-testable without a native library.
//!
//! IMPORTANT (transcribe.h): the mask must be fixed before any device
//! query or model load — call `TranscribeCpp::init_backends(mask)` once,
//! with the mask from [`ComputeMode::allowed_mask`], BEFORE using
//! [`select_device`].

use std::os::raw::c_int;

/// The compute backend the app asks the engine to run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeMode {
    /// Let the library pick the best device (discrete GPU → iGPU → CPU).
    Auto,
    /// Require the Vulkan backend; falls back to CPU at load time when the
    /// Vulkan module could not register (missing driver), surfaced via
    /// [`DeviceSelection::fell_back_to_cpu`].
    Vulkan,
    /// Strict CPU only.
    Cpu,
}

impl ComputeMode {
    /// The `TRANSCRIBE_BACKEND_MASK_*` mask to fix at
    /// `transcribe_init_backends_ex`. CPU is always allowed by the library
    /// itself; the mask decides whether GPU modules are ever opened.
    pub fn allowed_mask(self) -> u32 {
        use super::ffi;
        match self {
            Self::Auto => ffi::TRANSCRIBE_BACKEND_MASK_ALL,
            Self::Vulkan => ffi::TRANSCRIBE_BACKEND_MASK_VULKAN,
            Self::Cpu => ffi::TRANSCRIBE_BACKEND_MASK_CPU,
        }
    }

    /// The `transcribe_backend_request` value for
    /// `transcribe_model_load_params::backend`.
    pub fn backend_request(self) -> c_int {
        use super::ffi;
        match self {
            Self::Auto => ffi::TRANSCRIBE_BACKEND_AUTO,
            Self::Vulkan => ffi::TRANSCRIBE_BACKEND_VULKAN,
            Self::Cpu => ffi::TRANSCRIBE_BACKEND_CPU,
        }
    }

    /// The device `kind` string this mode selects (`transcribe_device_info::kind`);
    /// `None` for Auto, which accepts whatever the library ranks best.
    pub fn device_kind_filter(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Vulkan => Some("vulkan"),
            Self::Cpu => Some("cpu"),
        }
    }

    /// Stable token for configuration storage.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Vulkan => "vulkan",
            Self::Cpu => "cpu",
        }
    }

    /// Parses a configuration token; unknown or `None` inputs mean Auto.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()) {
            Some(ref v) if v == "vulkan" => Self::Vulkan,
            Some(ref v) if v == "cpu" => Self::Cpu,
            _ => Self::Auto,
        }
    }
}

/// The read-only device-registry view [`select_device`] needs, abstracted
/// for tests (implemented by [`super::wrapper::TranscribeCpp`]).
pub trait DeviceQuery {
    /// `transcribe_backend_available(kind)`.
    fn backend_available(&self, kind: c_int) -> bool;
    /// `transcribe_device_count()`.
    fn device_count(&self) -> i32;
    /// `transcribe_device_info::kind` of the device at `index`; None when
    /// out of range or unreported.
    fn device_kind(&self, index: i32) -> Option<String>;
}

/// The device selection handed to `TranscribeCpp::load_model`: which
/// backend to request and which registered device (by index) to pin, if
/// any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSelection {
    /// The mode the user/app asked for.
    pub mode: ComputeMode,
    /// The `transcribe_backend_request` to put in the model load params.
    pub backend_request: c_int,
    /// Index into the device registry; `None` = pass NULL and let the
    /// library auto-select for the requested backend.
    pub device_index: Option<i32>,
    /// True when Vulkan was requested but could not register and the
    /// selection degraded to CPU (ticket 06 uses this to warn about a
    /// Vulkan→CPU fallback; compare with `Model::backend()` after load).
    pub fell_back_to_cpu: bool,
}

/// Picks a device for `mode` from the (already-initialized) registry.
///
/// When the requested backend is unavailable — e.g. the Vulkan module was
/// skipped for a missing driver — the selection degrades to CPU instead of
/// failing, matching the library's designed degradation. The returned
/// [`DeviceSelection::fell_back_to_cpu`] records that it happened.
pub fn select_device(mode: ComputeMode, devices: &dyn DeviceQuery) -> DeviceSelection {
    let mut backend_request = mode.backend_request();
    let mut fell_back_to_cpu = false;

    // An explicit GPU mode only makes sense if some device of that kind
    // registered; otherwise request strict CPU (the load would fail with
    // TRANSCRIBE_ERR_BACKEND otherwise).
    if mode == ComputeMode::Vulkan && !devices.backend_available(backend_request) {
        backend_request = super::ffi::TRANSCRIBE_BACKEND_CPU;
        fell_back_to_cpu = true;
    }

    let filter = mode.device_kind_filter().filter(|_| !fell_back_to_cpu);
    let device_index = match filter {
        Some(kind) => {
            let count = devices.device_count().max(0);
            (0..count).find(|&i| devices.device_kind(i).as_deref() == Some(kind))
        }
        None => None,
    };

    DeviceSelection {
        mode,
        backend_request,
        device_index,
        fell_back_to_cpu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe_cpp_engine::ffi;

    struct FakeDevices {
        available: Vec<c_int>,
        kinds: Vec<String>,
    }

    impl FakeDevices {
        fn new(available: Vec<c_int>, kinds: Vec<&str>) -> Self {
            Self {
                available,
                kinds: kinds.into_iter().map(str::to_string).collect(),
            }
        }
    }

    impl DeviceQuery for FakeDevices {
        fn backend_available(&self, kind: c_int) -> bool {
            self.available.contains(&kind)
        }
        fn device_count(&self) -> i32 {
            self.kinds.len() as i32
        }
        fn device_kind(&self, index: i32) -> Option<String> {
            self.kinds.get(index as usize).cloned()
        }
    }

    #[test]
    fn modes_map_to_masks_backend_requests_and_kind_filters() {
        // Independent source of truth: transcribe.h v0.4.2 mask and
        // backend-request enums + device kind strings.
        assert_eq!(ComputeMode::Auto.allowed_mask(), 0xFFFF_FFFF);
        assert_eq!(ComputeMode::Vulkan.allowed_mask(), 0x0000_0004);
        assert_eq!(ComputeMode::Cpu.allowed_mask(), 0x0000_0001);

        assert_eq!(ComputeMode::Auto.backend_request(), ffi::TRANSCRIBE_BACKEND_AUTO);
        assert_eq!(
            ComputeMode::Vulkan.backend_request(),
            ffi::TRANSCRIBE_BACKEND_VULKAN
        );
        assert_eq!(ComputeMode::Cpu.backend_request(), ffi::TRANSCRIBE_BACKEND_CPU);

        assert_eq!(ComputeMode::Auto.device_kind_filter(), None);
        assert_eq!(ComputeMode::Vulkan.device_kind_filter(), Some("vulkan"));
        assert_eq!(ComputeMode::Cpu.device_kind_filter(), Some("cpu"));
    }

    #[test]
    fn auto_selects_no_device_and_never_falls_back() {
        let devices = FakeDevices::new(vec![ffi::TRANSCRIBE_BACKEND_AUTO], vec!["cpu"]);
        let selection = select_device(ComputeMode::Auto, &devices);
        assert_eq!(selection.backend_request, ffi::TRANSCRIBE_BACKEND_AUTO);
        assert_eq!(selection.device_index, None);
        assert!(!selection.fell_back_to_cpu);
    }

    #[test]
    fn vulkan_picks_the_first_vulkan_device() {
        let devices = FakeDevices::new(
            vec![ffi::TRANSCRIBE_BACKEND_CPU, ffi::TRANSCRIBE_BACKEND_VULKAN],
            vec!["cpu", "cpu", "vulkan"],
        );
        let selection = select_device(ComputeMode::Vulkan, &devices);
        assert_eq!(selection.backend_request, ffi::TRANSCRIBE_BACKEND_VULKAN);
        assert_eq!(selection.device_index, Some(2));
        assert!(!selection.fell_back_to_cpu);
    }

    #[test]
    fn vulkan_without_a_registered_vulkan_degrades_to_cpu() {
        // The Vulkan module was skipped (no driver): backend_available
        // answers false and only cpu devices registered.
        let devices = FakeDevices::new(vec![ffi::TRANSCRIBE_BACKEND_CPU], vec!["cpu"]);
        let selection = select_device(ComputeMode::Vulkan, &devices);
        assert_eq!(selection.backend_request, ffi::TRANSCRIBE_BACKEND_CPU);
        assert_eq!(selection.device_index, None);
        assert!(selection.fell_back_to_cpu);
    }

    #[test]
    fn cpu_picks_the_first_cpu_device() {
        let devices = FakeDevices::new(
            vec![ffi::TRANSCRIBE_BACKEND_CPU],
            vec!["accel", "cpu", "vulkan"],
        );
        let selection = select_device(ComputeMode::Cpu, &devices);
        assert_eq!(selection.backend_request, ffi::TRANSCRIBE_BACKEND_CPU);
        assert_eq!(selection.device_index, Some(1));
        assert!(!selection.fell_back_to_cpu);
    }

    #[test]
    fn cpu_without_a_kind_tagged_cpu_device_stays_on_automatic_selection() {
        // Defensive: an untagged registry (kind == "unknown") still loads
        // on CPU with the library's own selection policy rather than
        // erroring.
        let devices = FakeDevices::new(vec![ffi::TRANSCRIBE_BACKEND_CPU], vec!["unknown"]);
        let selection = select_device(ComputeMode::Cpu, &devices);
        assert_eq!(selection.backend_request, ffi::TRANSCRIBE_BACKEND_CPU);
        assert_eq!(selection.device_index, None);
    }

    #[test]
    fn config_tokens_round_trip_and_unknown_means_auto() {
        for mode in [ComputeMode::Auto, ComputeMode::Vulkan, ComputeMode::Cpu] {
            assert_eq!(ComputeMode::parse(Some(mode.as_str())), mode);
        }
        assert_eq!(ComputeMode::parse(Some(" Vulkan ")), ComputeMode::Vulkan);
        assert_eq!(ComputeMode::parse(Some("cuda")), ComputeMode::Auto);
        assert_eq!(ComputeMode::parse(None), ComputeMode::Auto);
    }
}
