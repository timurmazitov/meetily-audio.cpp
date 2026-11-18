//! Meeting summary template management
//!
//! This module provides a flexible template system for generating meeting summaries.
//! It supports both built-in templates (embedded in the binary) and custom user templates
//! (loaded from the application data directory).
//!
//! # Architecture
//!
//! - **Built-in templates**: JSON files in `frontend/src-tauri/templates/` embedded at compile time
//! - **Custom templates**: JSON files in platform-specific app data directory
//! - **Fallback strategy**: Custom templates override built-in templates with the same ID
//!
//! # Usage
//!
//! ```rust
//! use app_lib::summary::templates;
//!
//! // Load a specific template
//! let template = templates::get_template("daily_standup").expect("daily standup template exists");
//!
//! // Generate markdown structure
//! let markdown = template.to_markdown_structure();
//!
//! // Generate LLM instructions
//! let instructions = template.to_section_instructions();
//!
//! // List available templates
//! let available = templates::list_templates();
//! ```
//!
//! # Custom Templates
//!
//! Users can add custom templates to:
//! - macOS: `~/Library/Application Support/Samobranka/templates/`
//! - Windows: `%APPDATA%\Samobranka\templates\`
//! - Linux: `~/.config/Samobranka/templates/`
//!
//! Custom templates must follow the JSON schema defined in `types::Template`.

mod defaults;
mod loader;
mod types;

// Re-export public API
pub use loader::{
    get_template, list_template_ids, list_templates, set_bundled_templates_dir,
    validate_and_parse_template,
};
pub use types::{Template, TemplateSection};

#[cfg(test)]
mod tests {
    use super::*;

    /// Every bundled template file, keyed by file stem.
    const BUNDLED_TEMPLATE_FILES: [&str; 6] = [
        "daily_standup",
        "project_sync",
        "psychatric_session",
        "retrospective",
        "sales_marketing_client_call",
        "standard_meeting",
    ];

    /// The section that reports the meeting date, per bundled template.
    ///
    /// `retrospective` asks for a sprint range, not the meeting date; it still has
    /// to point at the date block so the model does not substitute the meeting
    /// date for the sprint range.
    const DATE_SECTIONS: [(&str, &str); 5] = [
        ("daily_standup", "Date"),
        ("project_sync", "Meeting Date & Time"),
        ("psychatric_session", "Session Metadata"),
        ("sales_marketing_client_call", "Meeting Metadata"),
        ("retrospective", "Sprint"),
    ];

    /// Reads a bundled template straight from the source tree.
    ///
    /// Four of the six templates are only shipped as resources, so
    /// `list_template_ids()`/`get_template()` cannot reach them in a bare `cargo test`.
    fn read_bundled_template(id: &str) -> String {
        let path = format!("{}/templates/{}.json", env!("CARGO_MANIFEST_DIR"), id);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read bundled template '{}': {}", path, e))
    }

    fn parsed_bundled_template(id: &str) -> Template {
        validate_and_parse_template(&read_bundled_template(id))
            .unwrap_or_else(|e| panic!("Bundled template '{}' is invalid: {}", id, e))
    }

    #[test]
    fn every_bundled_template_file_parses_and_validates() {
        for id in BUNDLED_TEMPLATE_FILES {
            let template = parsed_bundled_template(id);
            assert!(
                !template.sections.is_empty(),
                "Bundled template '{}' has no sections",
                id
            );
        }
    }

    fn date_section(id: &str, section_title: &str) -> TemplateSection {
        parsed_bundled_template(id)
            .sections
            .into_iter()
            .find(|section| section.title == section_title)
            .unwrap_or_else(|| {
                panic!(
                    "Bundled template '{}' has no '{}' section",
                    id, section_title
                )
            })
    }

    #[test]
    fn date_section_takes_the_date_from_the_meeting_date_block() {
        for (id, section_title) in DATE_SECTIONS {
            let section = date_section(id, section_title);

            assert!(
                section.instruction.contains("<meeting_date>"),
                "Instruction of '{}' in '{}' does not point at the date block: {:?}",
                section_title,
                id,
                section.instruction
            );
        }
    }

    #[test]
    fn date_section_forbids_inventing_a_missing_date() {
        for (id, section_title) in DATE_SECTIONS {
            let section = date_section(id, section_title);
            let instruction = section.instruction.to_lowercase();

            assert!(
                instruction.contains("do not invent"),
                "Instruction of '{}' in '{}' does not forbid inventing a date: {:?}",
                section_title,
                id,
                section.instruction
            );
            assert!(
                instruction.contains("unknown"),
                "Instruction of '{}' in '{}' does not ask for an unknown date: {:?}",
                section_title,
                id,
                section.instruction
            );
        }
    }

    #[test]
    fn only_the_date_section_of_a_template_points_at_the_date_block() {
        // Other date-ish instructions (follow-up dates, due dates, ETAs) concern
        // other events and must not be redirected to the meeting date.
        for (id, _) in DATE_SECTIONS {
            let referencing: Vec<String> = parsed_bundled_template(id)
                .sections
                .into_iter()
                .filter(|section| section.instruction.contains("<meeting_date>"))
                .map(|section| section.title)
                .collect();

            assert_eq!(
                referencing.len(),
                1,
                "Exactly one section of '{}' may reference the date block, found {:?}",
                id,
                referencing
            );
        }
    }

    #[test]
    fn retrospective_keeps_the_sprint_range_separate_from_the_meeting_date() {
        // The Sprint section asks for a sprint date range, not the meeting date:
        // the date block must never be used to fill the range.
        let instruction = date_section("retrospective", "Sprint").instruction;

        assert!(
            !instruction.to_lowercase().contains("verbatim"),
            "retrospective must not take the sprint range from the date block: {:?}",
            instruction
        );
        assert!(
            instruction.contains("not a substitute"),
            "retrospective must say the meeting date is not a substitute for the sprint range: {:?}",
            instruction
        );
    }

    #[test]
    fn daily_standup_keeps_the_date_format_its_section_asks_for() {
        let template = parsed_bundled_template("daily_standup");
        let section = template
            .sections
            .iter()
            .find(|section| section.title == "Date")
            .expect("daily_standup has a Date section");

        assert!(
            section.instruction.contains("YYYY-MM-DD"),
            "daily_standup still has to report the date as YYYY-MM-DD: {:?}",
            section.instruction
        );
    }

    #[test]
    fn default_system_prompt_does_not_reference_the_meeting_date_block() {
        // The base prompt is user-editable, so the date must never be wired
        // through it: users with a saved prompt would silently lose the date.
        let prompt = crate::summary::system_prompt::DEFAULT_SUMMARY_SYSTEM_PROMPT;
        assert!(!prompt.contains("<meeting_date>"));
    }

    #[test]
    fn test_module_integration() {
        // Test that we can load all built-in templates
        let ids = list_template_ids();
        assert!(!ids.is_empty());

        for id in ids {
            let result = get_template(&id);
            assert!(
                result.is_ok(),
                "Failed to load template '{}': {:?}",
                id,
                result.err()
            );
        }
    }

    #[test]
    fn test_template_metadata() {
        let templates = list_templates();
        assert!(!templates.is_empty());

        for (id, name, description) in templates {
            assert!(!id.is_empty());
            assert!(!name.is_empty());
            assert!(!description.is_empty());
        }
    }
}
