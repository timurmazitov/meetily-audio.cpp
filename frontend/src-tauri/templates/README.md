# Meeting Summary Templates

This directory contains template definitions for meeting summary generation.

## Available Templates

### 1. `daily_standup.json`
Time-boxed daily updates template designed for engineering/product teams.

**Sections:**
- Date
- Attendees
- Yesterday (completed work)
- Today (planned work)
- Blockers
- Notes

### 2. `standard_meeting.json`
General-purpose meeting notes template focusing on key outcomes and actions.

**Sections:**
- Summary
- Key Decisions
- Action Items
- Discussion Highlights

## Template Structure

Each template JSON file follows this schema:

```json
{
  "name": "Template Name",
  "description": "Brief description of the template's purpose",
  "system_prompt": "Optional: replaces the base system prompt. Supports {section_instructions} and {template}.",
  "sections": [
    {
      "title": "Section Title",
      "instruction": "Instructions for the LLM on what to extract/include",
      "format": "paragraph|list|string",
      "item_format": "Optional: Markdown table format for list items"
    }
  ]
}
```

## Custom Templates

Users can add custom templates to the application data directory:

- **macOS**: `~/Library/Application Support/Samobranka/templates/`
- **Windows**: `%APPDATA%\Samobranka\templates\`
- **Linux**: `~/.config/Samobranka/templates/`

Custom templates override built-in templates with the same filename.

## Template Fields

### Root Level
- `name` (required): Display name for the template
- `description` (required): Brief explanation of the template's use case
- `sections` (required): Array of section definitions
- `system_prompt` (optional): Custom system prompt for this template. When set, it replaces the base system prompt configured in Settings → Summary. Both `{section_instructions}` and `{template}` placeholders are supported (same as the base prompt).

### Section Object
- `title` (required): Section heading text
- `instruction` (required): LLM guidance for this section
- `format` (required): One of `"paragraph"`, `"list"`, or `"string"`
- `item_format` (optional): Markdown formatting hint for list items (e.g., table structure)
- `example_item_format` (optional): Alternative formatting hint

## Usage in Code

Templates are loaded using the `templates` module:

```rust
use crate::summary::templates;

// Get a specific template
let template = templates::get_template("daily_standup")?;

// List available templates
let available = templates::list_templates();

// Validate custom template JSON
let custom_json = std::fs::read_to_string("custom.json")?;
let validated = templates::validate_template(&custom_json)?;
```

## Meeting Date in Report Sections

Sections that report the meeting date (`Date` in `daily_standup.json`, `Meeting Date & Time` in
`project_sync.json`, `Session Metadata` in `psychatric_session.json`, `Meeting Metadata` in
`sales_marketing_client_call.json`) take it from the `<meeting_date>...</meeting_date>` block at
the start of the final generation request, not from the transcript. Each instruction states that a
missing block must not be invented around: the report says the date is unknown instead.

`Sprint` in `retrospective.json` is different: it asks for a sprint date range, which still comes
from the transcript. The meeting date is explicitly not a substitute for that range.

Note for release notes: this change only touches the bundled templates. A user who already copied a
template into their own templates directory keeps their copy (custom templates win over bundled
ones), so the updated date instructions do not reach it.
