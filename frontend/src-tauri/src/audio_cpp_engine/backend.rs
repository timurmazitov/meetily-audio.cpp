//! Compute-backend selection for the audio.cpp engine.
//!
//! The engine picks its compute backend with the app's existing hardware
//! autodetect (priority CUDA → Vulkan → CPU), exactly like the Whisper
//! acceleration rules (`whisper_engine::acceleration`). The pinned audio.cpp
//! release archives ship `cpu` and `vulkan` library variants
//! (`lib-<platform>-cpu`, `lib-<platform>-vulkan`), so a CUDA-detected
//! machine is served by the Vulkan variant, which accelerates on NVIDIA
//! GPUs through the vendor Vulkan ICD. Everything in this module is a pure
//! function of the detected [`GpuType`], so the selection is tested without
//! a native library.
//!
//! Downloaded models are backend-agnostic: every variant loads the same
//! GGUF catalog, so switching the compute backend never invalidates an
//! installed model.

use crate::audio::{GpuType, HardwareProfile};

use super::wrapper::BackendConfig;

/// The compute backend an audio.cpp session runs on. Mirrors the
/// `audiocpp_backend_config.backend` vocabulary of the pinned C ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCppBackend {
    Cuda,
    Vulkan,
    Cpu,
}

impl AudioCppBackend {
    /// The backend the detected GPU asks for, using the app's existing
    /// autodetect priority: CUDA (NVIDIA) → Vulkan (AMD/Intel) → CPU.
    /// GPU types without a pinned library variant (Metal, OpenCL) fall
    /// back to CPU so the app keeps working everywhere.
    pub fn autodetect(gpu: GpuType) -> Self {
        match gpu {
            GpuType::Cuda => Self::Cuda,
            GpuType::Vulkan => Self::Vulkan,
            GpuType::Metal | GpuType::OpenCL | GpuType::None => Self::Cpu,
        }
    }

    /// Session token for `audiocpp_backend_config.backend`
    /// (docs/research/audio-cpp-c-api.md).
    pub fn session_token(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Vulkan => "vulkan",
            Self::Cpu => "cpu",
        }
    }

    /// Human-readable status label, in the style of the Whisper
    /// acceleration labels.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cuda => "CUDA GPU acceleration",
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

/// The library variant to load for a desired backend. The pinned release
/// archives ship `cpu` and `vulkan` variants only, so a CUDA desire is
/// served by the Vulkan variant (Vulkan runs on NVIDIA GPUs through the
/// vendor ICD); Vulkan and CPU pass through unchanged.
pub fn shipped_variant(desired: AudioCppBackend) -> AudioCppBackend {
    match desired {
        AudioCppBackend::Cuda => AudioCppBackend::Vulkan,
        other => other,
    }
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
    fn autodetect_prefers_cuda_for_an_nvidia_gpu() {
        assert_eq!(AudioCppBackend::autodetect(GpuType::Cuda), AudioCppBackend::Cuda);
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
    fn shipped_variant_serves_cuda_from_the_vulkan_archive() {
        // The pinned release archives ship cpu and vulkan library variants;
        // Vulkan accelerates on NVIDIA GPUs through the vendor ICD.
        assert_eq!(
            shipped_variant(AudioCppBackend::Cuda),
            AudioCppBackend::Vulkan
        );
    }

    #[test]
    fn shipped_variant_keeps_vulkan_and_cpu() {
        assert_eq!(
            shipped_variant(AudioCppBackend::Vulkan),
            AudioCppBackend::Vulkan
        );
        assert_eq!(shipped_variant(AudioCppBackend::Cpu), AudioCppBackend::Cpu);
    }

    #[test]
    fn session_tokens_match_the_audio_cpp_backend_vocabulary() {
        // Independent source of truth: docs/research/audio-cpp-c-api.md
        // (audiocpp_backend_config.backend: "cpu", "cuda", "vulkan").
        assert_eq!(AudioCppBackend::Cuda.session_token(), "cuda");
        assert_eq!(AudioCppBackend::Vulkan.session_token(), "vulkan");
        assert_eq!(AudioCppBackend::Cpu.session_token(), "cpu");
    }

    #[test]
    fn session_config_requests_the_backend_token_with_library_defaults() {
        for backend in [AudioCppBackend::Cuda, AudioCppBackend::Vulkan, AudioCppBackend::Cpu] {
            let config = session_config(backend);
            assert_eq!(config.backend.as_deref(), Some(backend.session_token()));
            assert_eq!(config.device, 0);
            assert_eq!(config.threads, 0);
        }
    }
}
