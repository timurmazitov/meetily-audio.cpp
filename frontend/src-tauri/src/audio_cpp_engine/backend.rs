//! Compute-backend selection for the audio.cpp engine.
//!
//! The engine picks its compute backend with the app's existing hardware
//! autodetect (priority CUDA → Vulkan → CPU), exactly like the Whisper
//! acceleration rules (`whisper_engine::acceleration`). The pinned audio.cpp
//! release archives ship `cpu` and `vulkan` library variants only
//! (`lib-<platform>-cpu`, `lib-<platform>-vulkan`), so an NVIDIA machine is
//! served by the Vulkan variant, which accelerates on NVIDIA GPUs through
//! the vendor Vulkan ICD. There is no CUDA library variant in the pin, so
//! this module deliberately has no CUDA value: advertising one would
//! describe a codepath that does not exist.
//!
//! Everything in this module is a pure function of the detected [`GpuType`],
//! so the selection is tested without a native library.
//!
//! Downloaded models are backend-agnostic: every variant loads the same
//! GGUF catalog, so switching the compute backend never invalidates an
//! installed model.

use crate::audio::{GpuType, HardwareProfile};

use super::wrapper::BackendConfig;

/// The compute backend an audio.cpp session runs on, limited to the library
/// variants the pinned archives actually ship (the C ABI vocabulary also
/// knows `"cuda"`, but no pinned archive provides that flavour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCppBackend {
    Vulkan,
    Cpu,
}

impl AudioCppBackend {
    /// The backend the detected GPU asks for, using the app's existing
    /// autodetect priority: CUDA (NVIDIA) → Vulkan (AMD/Intel) → CPU.
    ///
    /// NVIDIA is served by the Vulkan variant because the pinned archives
    /// ship no CUDA flavour; Vulkan runs on NVIDIA through the vendor ICD.
    /// GPU types without a pinned library variant (Metal, OpenCL) fall back
    /// to CPU so the app keeps working everywhere.
    pub fn autodetect(gpu: GpuType) -> Self {
        match gpu {
            GpuType::Cuda | GpuType::Vulkan => Self::Vulkan,
            GpuType::Metal | GpuType::OpenCL | GpuType::None => Self::Cpu,
        }
    }

    /// Session token for `audiocpp_backend_config.backend`
    /// (docs/research/audio-cpp-c-api.md). Only tokens whose library
    /// variant ships in the pinned release are reachable here.
    pub fn session_token(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::Cpu => "cpu",
        }
    }

    /// Human-readable status label, in the style of the Whisper
    /// acceleration labels.
    pub fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan GPU acceleration",
            Self::Cpu => "CPU processing only",
        }
    }
}

/// Detects the backend for this machine via the shared hardware profile
/// (cached after the first call).
pub fn autodetect_backend() -> AudioCppBackend {
    AudioCppBackend::autodetect(HardwareProfile::detect().gpu_type)
}

/// The `audiocpp_backend_config` for a session on the given backend:
/// explicit backend token, default device, library-picked thread count
/// (the library never touches the global OpenMP thread count).
pub fn session_config(backend: AudioCppBackend) -> BackendConfig {
    BackendConfig {
        backend: Some(backend.session_token().to_string()),
        device: 0,
        threads: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_nvidia_gpu_is_served_by_the_vulkan_variant() {
        // The pin ships cpu and vulkan archives only; Vulkan accelerates on
        // NVIDIA GPUs through the vendor ICD.
        assert_eq!(
            AudioCppBackend::autodetect(GpuType::Cuda),
            AudioCppBackend::Vulkan
        );
    }

    #[test]
    fn autodetect_uses_vulkan_for_a_non_nvidia_gpu() {
        assert_eq!(
            AudioCppBackend::autodetect(GpuType::Vulkan),
            AudioCppBackend::Vulkan
        );
    }

    #[test]
    fn autodetect_falls_back_to_cpu_without_a_supported_gpu() {
        for gpu in [GpuType::None, GpuType::Metal, GpuType::OpenCL] {
            assert_eq!(AudioCppBackend::autodetect(gpu), AudioCppBackend::Cpu);
        }
    }

    #[test]
    fn session_tokens_match_the_shipped_library_variants() {
        // Independent source of truth: the pinned release archives
        // (`lib-<platform>-cpu`, `lib-<platform>-vulkan`). No CUDA archive
        // exists in the pin, so no CUDA token is reachable.
        assert_eq!(AudioCppBackend::Vulkan.session_token(), "vulkan");
        assert_eq!(AudioCppBackend::Cpu.session_token(), "cpu");
    }

    #[test]
    fn session_config_requests_the_backend_token_with_library_defaults() {
        for backend in [AudioCppBackend::Vulkan, AudioCppBackend::Cpu] {
            let config = session_config(backend);
            assert_eq!(config.backend.as_deref(), Some(backend.session_token()));
            assert_eq!(config.device, 0);
            assert_eq!(config.threads, 0);
        }
    }
}
