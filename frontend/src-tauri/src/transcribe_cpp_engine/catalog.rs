//! Static catalog of the models the transcribe.cpp provider can use.
//!
//! The catalog is deliberately free of any engine interaction: it describes
//! what a model *is* — its transcribe.cpp family (the catalog variant), the
//! quantization of the published weights, the exact artifact size, the pinned
//! SHA-256 digest and where the weights live upstream. Nothing here loads the
//! native library, so a model can be listed, downloaded, validated and
//! deleted before transcription support exists.
//!
//! Every value below was verified against the Hugging Face API
//! (`api/models/<repo>` for the pinned commit, `tree/main` for the LFS
//! `oid`/`size` of each file) on 2026-11-20. Sources are pinned to a commit
//! so that the artifact a user downloads is the artifact the catalog
//! describes: `https://huggingface.co/handy-computer/<repo>/resolve/<commit>/<filename>`.
//!
//! Only the quantizations the app offers are listed: F16, Q8_0, Q6_K, Q5_K_M
//! and Q4_K_M. F32 is skipped (it is never a reasonable download for the app)
//! and so is the Qwen BF16 duplicate (F16 exists for that variant). The
//! Whisper families do not publish F32 at all.
//!
//! Model ids are the artifact filename without its `.gguf` suffix, e.g.
//! `parakeet-ultra-Q8_0`. Note that the Qwen package keeps its upstream
//! capitalization (`Qwen3-ASR-0.6B-Q8_0`) while its family stays lowercase.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Installation state of a catalogued model. Mirrors the shape the Whisper,
/// Parakeet and audio.cpp managers expose so the frontend model managers stay
/// consistent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ModelStatus {
    Available,
    Missing,
    Downloading {
        progress: u8,
    },
    Error(String),
    Corrupted {
        file_size: u64,
        expected_min_size: u64,
    },
}

/// A catalogued model plus its on-disk state.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Stable model id used by the commands and the UI: the filename stem.
    pub name: String,
    /// Human-readable model name.
    pub display_name: String,
    /// transcribe.cpp model family (the catalog variant), e.g. `parakeet-ultra`.
    pub family: String,
    /// Weight quantization as published upstream, e.g. `F16` or `Q8_0`.
    pub quantization: String,
    /// File name inside the transcribe.cpp models directory.
    pub filename: String,
    /// Full path of the local artifact.
    pub path: PathBuf,
    /// Size in MiB, for display.
    pub size_mb: u32,
    /// Exact artifact size in bytes; downloads are validated against it.
    pub size_bytes: u64,
    /// Catalog description (purpose and trade-offs).
    pub description: String,
    /// SHA-256 digest of the artifact as published upstream, lowercase hex.
    pub sha256: String,
    /// Current installation state.
    pub status: ModelStatus,
}

/// Static description of one supported model.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub name: &'static str,
    pub display_name: &'static str,
    pub family: &'static str,
    pub quantization: &'static str,
    pub filename: &'static str,
    pub size_mb: u32,
    pub exact_bytes: u64,
    pub description: &'static str,
    /// Lowercase hex SHA-256 as reported by the Hugging Face LFS metadata.
    pub sha256: &'static str,
    pub source_url: &'static str,
}

impl ModelSpec {
    /// Where this model's artifact lives inside `models_dir`.
    pub fn file_path(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.filename)
    }

    /// Build the serializable description of this model for a given
    /// on-disk status.
    pub fn model_info(&self, models_dir: &Path, status: ModelStatus) -> ModelInfo {
        ModelInfo {
            name: self.name.to_string(),
            display_name: self.display_name.to_string(),
            family: self.family.to_string(),
            quantization: self.quantization.to_string(),
            filename: self.filename.to_string(),
            path: self.file_path(models_dir),
            size_mb: self.size_mb,
            size_bytes: self.exact_bytes,
            description: self.description.to_string(),
            sha256: self.sha256.to_string(),
            status,
        }
    }
}

/// Builds the catalog from variant declarations. Each variant lists its
/// offered quantizations as `QUANT => { bytes, size_mb, sha256 }`; the model
/// id and filename are built as `<slug>-<QUANT>` / `<slug>-<QUANT>.gguf`, so
/// the id is the filename stem by construction and the source URL pins the
/// published repository revision. The `size_mb` display figure is
/// `bytes / 1 MiB` rounded; the test suite asserts that relationship against
/// the exact byte count, so the two can never drift apart silently.
macro_rules! model_catalog {
    (
        $(
            slug: $slug:expr,
            display: $display:expr,
            family: $family:expr,
            repo: $repo:expr,
            commit: $commit:expr,
            description: $description:expr,
            [ $( $quant:ident => { bytes: $bytes:expr, size_mb: $size_mb:expr, sha256: $sha256:expr } ),* $(,)? ]
        ),* $(,)?
    ) => {
        &[ $( $(
            ModelSpec {
                name: concat!($slug, "-", stringify!($quant)),
                display_name: $display,
                family: $family,
                quantization: stringify!($quant),
                filename: concat!($slug, "-", stringify!($quant), ".gguf"),
                size_mb: $size_mb,
                exact_bytes: $bytes,
                description: concat!($description, " Quantization: ", stringify!($quant), "."),
                sha256: $sha256,
                source_url: concat!(
                    "https://huggingface.co/handy-computer/",
                    $repo,
                    "/resolve/",
                    $commit,
                    "/",
                    $slug,
                    "-",
                    stringify!($quant),
                    ".gguf"
                ),
            }
        ),* ),* ]
    };
}

/// Models supported by the transcribe.cpp provider, in display order:
/// Russian-first (GigaAM), then the multilingual variants, then Whisper.
/// 9 variants × 5 offered quantizations = 45 entries. Every entry carries
/// the exact artifact size and the SHA-256 digest pinned from the Hugging
/// Face LFS metadata of the listed commit (see the module docs).
pub const MODEL_CATALOG: &[ModelSpec] = model_catalog!(
    slug: "gigaam-v3-ctc",
    display: "GigaAM v3 CTC",
    family: "gigaam-v3-ctc",
    repo: "gigaam-v3-ctc-gguf",
    commit: "53ac479633748119e11b049cf28bc801f0440f10",
    description: "Russian speech recognition (GigaAM v3 CTC). Strong accuracy for Russian meetings.",
    [
        F16 => { bytes: 448_750_528, size_mb: 428, sha256: "1d958ddf73abf722e320ee36212190e9bdfc6bfdc89778d2e4deeaaeb5bce443" },
        Q8_0 => { bytes: 271_803_328, size_mb: 259, sha256: "71e5c82890e9e243a6bd7575f129f5d1bd2c3ca3ae79aab75cfb1a6934c6a62b" },
        Q6_K => { bytes: 226_091_968, size_mb: 216, sha256: "7556f1c8b9ed0f93b3e224d14a954b51cc331c5a3d2be0f5988bd0523b962c7f" },
        Q5_K_M => { bytes: 204_563_392, size_mb: 195, sha256: "2002cd7603b99c318b52a88047d6676e69218083014f19b11c84da0223fb2023" },
        Q4_K_M => { bytes: 182_150_080, size_mb: 174, sha256: "16b329fed3eaa197f2191506ac3cad72f46502297ed6a0f915c79d2ac08c404e" },
    ],
    slug: "gigaam-v3-e2e-ctc",
    display: "GigaAM v3 e2e CTC",
    family: "gigaam-v3-e2e-ctc",
    repo: "gigaam-v3-e2e-ctc-gguf",
    commit: "59717de63d50dd9dd93b3249da6ad3f2f1c4ad6b",
    description: "Russian speech recognition (GigaAM v3 end-to-end CTC). Best accuracy per size for Russian meetings.",
    [
        F16 => { bytes: 449_098_336, size_mb: 428, sha256: "25a2fd66a50912645307bca1489b8c07e9060ef079b61a7cce0041248cc89b4a" },
        Q8_0 => { bytes: 272_151_136, size_mb: 260, sha256: "9ccce4750dc813a493d96ca15ee251712bedec15ac9a02fa3d2bd732f08ae5eb" },
        Q6_K => { bytes: 226_439_776, size_mb: 216, sha256: "cb517247ae37589c5b281d6d9331ffb32124c77b789affe46a272be1428a5260" },
        Q5_K_M => { bytes: 204_911_200, size_mb: 195, sha256: "d7c6a729f9bae8714d05cb7e66149e662c5098fe10b66206b70f4917f4f84004" },
        Q4_K_M => { bytes: 182_497_888, size_mb: 174, sha256: "35581e11e048eef785657cff07e5fced794bba6d9c75143257f6452c8aeea655" },
    ],
    slug: "gigaam-v3-rnnt",
    display: "GigaAM v3 RNN-T",
    family: "gigaam-v3-rnnt",
    repo: "gigaam-v3-rnnt-gguf",
    commit: "93cba3cdf21d66ca7a746718c849419c2946d6bb",
    description: "Russian speech recognition (GigaAM v3 RNN-T). Better punctuation than the CTC variant.",
    [
        F16 => { bytes: 451_084_832, size_mb: 430, sha256: "b1fe43be121c64934e464c2320c576d74de1466cb7e91f2d8c2aa1149ebe8572" },
        Q8_0 => { bytes: 273_022_880, size_mb: 260, sha256: "130d54450be2922144b265d90d453b781fc2b5adaee52a16954b00f3686201f7" },
        Q6_K => { bytes: 227_252_000, size_mb: 217, sha256: "ee329ee6ef93b5da62e1915927278a9930b04ac95b2735e7c80e64b725f3f82d" },
        Q5_K_M => { bytes: 205_690_784, size_mb: 196, sha256: "3541b4e6b2c3909e99254bcb7c61565137038f24db5651a779885d0c08257348" },
        Q4_K_M => { bytes: 183_246_752, size_mb: 175, sha256: "33b09558330ccfdc5c156e6d6b57c89759a0ac3b653bcdfcd5f1e13f20fc1e76" },
    ],
    slug: "gigaam-v3-e2e-rnnt",
    display: "GigaAM v3 e2e RNN-T",
    family: "gigaam-v3-e2e-rnnt",
    repo: "gigaam-v3-e2e-rnnt-gguf",
    commit: "3efd5b39882ceadca5570a8fb9fa1a2c801d6d6d",
    description: "Russian speech recognition (GigaAM v3 end-to-end RNN-T). Punctuation-forward variant at full size.",
    [
        F16 => { bytes: 452_381_408, size_mb: 431, sha256: "87b12fa3523939d3c47c4b427f587e415dda41dcafa9e4ba17f2a1cab2f0d5ad" },
        Q8_0 => { bytes: 273_724_832, size_mb: 261, sha256: "78d63b47723b7f8d78c6113a6ef983b5a86e2a86f6c273e1f5cb6967b1c4467a" },
        Q6_K => { bytes: 227_953_952, size_mb: 217, sha256: "1ef0f771d9ab0b0675c45ce67a3e5b79ac7497fe35ebb08a51704c4ee423edce" },
        Q5_K_M => { bytes: 206_392_736, size_mb: 197, sha256: "0f5969b05f7e196e8922e243c64963132025a5ae7ec2f83d028ae120d201594f" },
        Q4_K_M => { bytes: 183_948_704, size_mb: 175, sha256: "7d69952fb431a8d7800ed9910dc61fea37d8406bfe96d10bf24c8bd4b7c68623" },
    ],
    slug: "parakeet-tdt-0.6b-v3",
    display: "Parakeet TDT 0.6B v3",
    family: "parakeet-tdt-0.6b-v3",
    repo: "parakeet-tdt-0.6b-v3-gguf",
    commit: "d8d8c32cb2017c57e5307767343e97870f8a7f53",
    description: "Multilingual speech recognition (NVIDIA Parakeet TDT 0.6B v3) with strong Russian and English accuracy.",
    [
        F16 => { bytes: 1_255_869_856, size_mb: 1198, sha256: "d9ec7e2c39da7b4fec3e01e71e82c7f7bbe97741e08c0c054091bf3d520a41d3" },
        Q8_0 => { bytes: 739_508_576, size_mb: 705, sha256: "5859f77944efcd8eafa23a6350731960b2b55b2203df51f319665c807d802cc7" },
        Q6_K => { bytes: 610_342_240, size_mb: 582, sha256: "ab1bbcbe3e1c4c2ec12ca1df1aeafd22e9043ce7262e4b5f5c5d7d76418a9ae8" },
        Q5_K_M => { bytes: 548_946_272, size_mb: 524, sha256: "cc722e76adc1a629fc0b2535de879d99b8160d07ad4c0215e2ca7d7ea0ae4b8f" },
        Q4_K_M => { bytes: 485_425_504, size_mb: 463, sha256: "b68557be1e3c40207fd7c4bd9d63f1d3316b963f15325bfb0cc16a8bb0ffd181" },
    ],
    slug: "parakeet-ultra",
    display: "Parakeet Ultra",
    family: "parakeet-ultra",
    repo: "parakeet-ultra-gguf",
    commit: "a1539be1b849ea7463b88343e5277b373d0c4ea9",
    description: "Multilingual speech recognition (Parakeet Ultra). Provider default: balanced Russian and English quality at a moderate size.",
    [
        F16 => { bytes: 1_256_724_448, size_mb: 1199, sha256: "cd97f94fa50a034a9019a47e2ed7a2de3963eed8316d1df4de9efdccca6fe78d" },
        Q8_0 => { bytes: 740_363_168, size_mb: 706, sha256: "007a59761e9258779f189df396b50d12b83bb5b79e66df4b955c230d2f2a0a59" },
        Q6_K => { bytes: 611_196_832, size_mb: 583, sha256: "ba146253d9b5eb7c729dfa897174cb5bad0a7d05129863805e0caf7d571c7493" },
        Q5_K_M => { bytes: 549_800_864, size_mb: 524, sha256: "f6e3d8172547a3bb468e92ff395696e6d2e0371a9e52690a27741b4e8561c997" },
        Q4_K_M => { bytes: 486_280_096, size_mb: 464, sha256: "c69b7a5f9071a7afd8a1818ef7ae542c386c4b01ed0cac380c19a99947646bf9" },
    ],
    slug: "Qwen3-ASR-0.6B",
    display: "Qwen3-ASR 0.6B",
    family: "qwen3-asr-0.6b",
    repo: "Qwen3-ASR-0.6B-gguf",
    commit: "7e7c69a577fa2725dfedbe34b5e999336e30a19b",
    description: "Multilingual speech recognition (Qwen3-ASR 0.6B), the strongest Russian accuracy in the catalog.",
    [
        F16 => { bytes: 1_579_793_056, size_mb: 1507, sha256: "5c90e4b1a72a4c59cd12afa5ebb0cc8628848148f2b337d025cc5121ae4d2eea" },
        Q8_0 => { bytes: 850_423_456, size_mb: 811, sha256: "f081b2d5e23bd669d92cc331d722a8a0681943b8e6f34b48996fd5c319b5acd8" },
        Q6_K => { bytes: 690_417_824, size_mb: 658, sha256: "3b051f108f03c0c91bbe1a3b2c1ee15e3ed51e4caec2a48751b01f2a21441cc3" },
        Q5_K_M => { bytes: 645_356_192, size_mb: 615, sha256: "062a7bcb18675c2fe5ffd0e8b354eac6a1ceada9b2af9ef7168746ae16b54358" },
        Q4_K_M => { bytes: 589_560_480, size_mb: 562, sha256: "5b58f32a58ffa2c8783e0b0963485623e286e6272d953dfc9e28bc3447dee0c0" },
    ],
    slug: "whisper-large-v3-turbo",
    display: "Whisper large-v3 Turbo",
    family: "whisper-large-v3-turbo",
    repo: "whisper-large-v3-turbo-gguf",
    commit: "18ada752005f5fddef499256f9a458d0b37a203a",
    description: "Multilingual speech recognition (OpenAI Whisper large-v3 Turbo), the fast Whisper fallback for long-tail languages.",
    [
        F16 => { bytes: 1_625_935_488, size_mb: 1551, sha256: "c8506ddfa3d4eb9279170d722a75422db00dbdbd7618531bbb81926e0598eb8f" },
        Q8_0 => { bytes: 886_381_728, size_mb: 845, sha256: "257aa7ec9121c22c167d5e2c49e3e76b4ba0e6e27ebc13074eba0a80f8190220" },
        Q6_K => { bytes: 692_536_896, size_mb: 660, sha256: "b8b4d57e1a6fc49bcd7843faadf224c27cb27707e13db7e6dbfbfffe39878103" },
        Q5_K_M => { bytes: 619_628_096, size_mb: 591, sha256: "06fc9923c09fb3ae878ea12c34d1be3ed18fad039ab14b4b045c7b6c40af56b2" },
        Q4_K_M => { bytes: 536_069_696, size_mb: 511, sha256: "c6e172d0074f0fa9440b88247854b50643cd4bb58b8515c797bbed16d858c7df" },
    ],
    slug: "whisper-large-v3",
    display: "Whisper large-v3",
    family: "whisper-large-v3",
    repo: "whisper-large-v3-gguf",
    commit: "274ad1d1cf680b7ee53cc70f78461394cd3a344b",
    description: "Multilingual speech recognition (OpenAI Whisper large-v3), the most accurate Whisper release.",
    [
        F16 => { bytes: 3_107_236_640, size_mb: 2963, sha256: "e633ab1d74b0e98f4f57daedaee34291297dbbf01389bda3d890766600c1c584" },
        Q8_0 => { bytes: 1_668_741_440, size_mb: 1591, sha256: "2fa1a5f179f8a511a53e2108db270aa4af3ce08cd976af4180e2854666bb4ba3" },
        Q6_K => { bytes: 1_297_130_208, size_mb: 1237, sha256: "88e6dc60bd0538da145a50c19f4778e4a106fe28713adeeca43163215ee02805" },
        Q5_K_M => { bytes: 1_161_143_008, size_mb: 1107, sha256: "6053d0fd69a0fd48b8fea5ea7a52b9e0cde389343566fa30e453a1b2b258dc38" },
        Q4_K_M => { bytes: 997_303_008, size_mb: 951, sha256: "6fe933811cec4cd3159debc46520ecd3aac6c7e322ece2ac61fcfdab184e1fe0" },
    ],
);

/// The full catalog.
pub fn catalog() -> &'static [ModelSpec] {
    MODEL_CATALOG
}

/// Look up a catalogued model by its stable name.
pub fn find_model(name: &str) -> Option<&'static ModelSpec> {
    MODEL_CATALOG.iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> &'static ModelSpec {
        find_model(name).unwrap_or_else(|| panic!("{name} must be catalogued"))
    }

    /// The nine catalog variants, in display order.
    const VARIANTS: [&str; 9] = [
        "gigaam-v3-ctc",
        "gigaam-v3-e2e-ctc",
        "gigaam-v3-rnnt",
        "gigaam-v3-e2e-rnnt",
        "parakeet-tdt-0.6b-v3",
        "parakeet-ultra",
        "qwen3-asr-0.6b",
        "whisper-large-v3-turbo",
        "whisper-large-v3",
    ];

    const QUANTS: [&str; 5] = ["F16", "Q8_0", "Q6_K", "Q5_K_M", "Q4_K_M"];

    #[test]
    fn catalog_lists_the_nine_variants_with_the_five_offered_quants() {
        assert_eq!(
            catalog().len(),
            VARIANTS.len() * QUANTS.len(),
            "9 variants x 5 quants = 45 entries"
        );

        for variant in VARIANTS {
            let entries: Vec<&ModelSpec> = catalog()
                .iter()
                .filter(|spec| spec.family == variant)
                .collect();
            assert_eq!(
                entries.len(),
                QUANTS.len(),
                "variant {variant} must offer exactly {:?}",
                QUANTS
            );
            for quant in QUANTS {
                assert!(
                    entries.iter().any(|spec| spec.quantization == quant),
                    "variant {variant} is missing {quant}"
                );
            }
        }
    }

    #[test]
    fn qwen_variant_is_listed_under_its_capitalised_package_but_lowercase_family() {
        let entry = spec("Qwen3-ASR-0.6B-Q8_0");

        assert_eq!(entry.family, "qwen3-asr-0.6b");
        assert_eq!(entry.quantization, "Q8_0");
        // The published package keeps upstream capitalization; no BF16 entry.
        assert_eq!(entry.filename, "Qwen3-ASR-0.6B-Q8_0.gguf");
        assert!(
            catalog()
                .iter()
                .all(|spec| spec.quantization != "BF16" && spec.quantization != "F32"),
            "only the five offered quants are catalogued"
        );
    }

    #[test]
    fn the_default_model_is_catalogued_with_the_default_quant() {
        let default = spec(crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL);

        assert_eq!(default.family, "parakeet-ultra");
        assert_eq!(default.quantization, "Q8_0");
        assert_eq!(default.filename, "parakeet-ultra-Q8_0.gguf");
    }

    #[test]
    fn model_id_matches_filename_stem() {
        for spec in catalog() {
            assert_eq!(
                spec.filename,
                format!("{}.gguf", spec.name),
                "model {} must be stored under its stable name",
                spec.name
            );
        }
    }

    #[test]
    fn size_mb_is_the_exact_byte_size_rounded_to_mib() {
        for spec in catalog() {
            let size_mib = (spec.exact_bytes as f64 / (1024.0 * 1024.0)).round() as u32;
            assert_eq!(spec.size_mb, size_mib, "model {} size_mb", spec.name);
        }
    }

    #[test]
    fn catalog_names_are_unique_and_models_are_found_by_name() {
        let mut names: Vec<&str> = catalog().iter().map(|spec| spec.name).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "catalog names must be unique");

        for spec in catalog() {
            assert!(find_model(spec.name).is_some());
        }
        assert!(find_model("whisper-large").is_none());
    }

    #[test]
    fn artifacts_are_pinned_to_commits_in_the_handy_computer_repositories() {
        for spec in catalog() {
            // https://huggingface.co/handy-computer/<repo>/resolve/<commit>/<filename>
            let rest = spec
                .source_url
                .strip_prefix("https://huggingface.co/handy-computer/")
                .unwrap_or_else(|| {
                    panic!("weights must come from handy-computer: {}", spec.source_url)
                });
            let (repo, tail) = rest.split_once("/resolve/").unwrap_or_else(|| {
                panic!("source url must pin a commit: {}", spec.source_url)
            });
            assert!(!repo.is_empty() && repo.ends_with("-gguf"), "repo: {repo}");

            let (commit, filename) = tail.split_once('/').unwrap_or_else(|| {
                panic!("source url must end with /<filename>: {}", spec.source_url)
            });
            assert_eq!(
                commit.len(),
                40,
                "commit must be a full SHA: {}",
                spec.source_url
            );
            assert!(
                commit.chars().all(|c| c.is_ascii_hexdigit()),
                "commit must be hex: {}",
                spec.source_url
            );
            assert_eq!(
                filename, spec.filename,
                "artifact {filename} does not match model {}",
                spec.name
            );
            assert!(
                spec.sha256.len() == 64 && spec.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "model {} must pin a lowercase hex SHA-256",
                spec.name
            );
        }
    }

    #[test]
    fn gigaam_weights_come_from_the_handy_computer_packages() {
        let entry = spec("gigaam-v3-e2e-ctc-Q8_0");

        assert!(
            entry.source_url.starts_with(
                "https://huggingface.co/handy-computer/gigaam-v3-e2e-ctc-gguf/resolve/"
            ),
            "unexpected source: {}",
            entry.source_url
        );
        assert!(entry
            .source_url
            .ends_with("/gigaam-v3-e2e-ctc-Q8_0.gguf"));
    }

    #[test]
    fn model_info_carries_family_quantization_and_digest_metadata() {
        let spec = spec("whisper-large-v3-Q4_K_M");
        let info = spec.model_info(Path::new("/models/transcribecpp"), ModelStatus::Missing);

        assert_eq!(info.name, "whisper-large-v3-Q4_K_M");
        assert_eq!(info.family, "whisper-large-v3");
        assert_eq!(info.quantization, "Q4_K_M");
        assert_eq!(info.filename, "whisper-large-v3-Q4_K_M.gguf");
        assert_eq!(
            info.path,
            PathBuf::from("/models/transcribecpp/whisper-large-v3-Q4_K_M.gguf")
        );
        assert_eq!(
            info.sha256,
            "6fe933811cec4cd3159debc46520ecd3aac6c7e322ece2ac61fcfdab184e1fe0"
        );
        assert_eq!(info.size_bytes, 997_303_008);
        assert!(matches!(info.status, ModelStatus::Missing));
    }

    #[test]
    fn status_serializes_in_the_shape_the_ui_expects() {
        let info = spec("parakeet-ultra-Q8_0").model_info(
            Path::new("/models"),
            ModelStatus::Downloading { progress: 7 },
        );
        let json = serde_json::to_value(&info).unwrap();

        assert_eq!(json["status"]["Downloading"]["progress"], 7);
        assert_eq!(json["quantization"], "Q8_0");

        let corrupted = serde_json::to_value(ModelStatus::Corrupted {
            file_size: 10,
            expected_min_size: 20,
        })
        .unwrap();
        assert_eq!(corrupted["Corrupted"]["file_size"], 10);
        assert_eq!(corrupted["Corrupted"]["expected_min_size"], 20);
    }
}
