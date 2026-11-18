export interface Message {
  id: string;
  content: string;
  timestamp: string;
}

export interface Transcript {
  id: string;
  text: string;
  timestamp: string; // Wall-clock time (e.g., "14:30:05")
  sequence_id?: number;
  chunk_start_time?: number; // Legacy field
  is_partial?: boolean;
  confidence?: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time?: number; // Seconds from recording start (e.g., 125.3)
  audio_end_time?: number;   // Seconds from recording start (e.g., 128.6)
  duration?: number;          // Segment duration in seconds (e.g., 3.3)
  /** Diarization speaker label (`speaker_0`, `speaker_1`, …); absent when not diarized. */
  speaker?: string;
  /** Assigned person's name for this segment's label; absent while unnamed. */
  speaker_name?: string;
}

export interface TranscriptUpdate {
  text: string;
  timestamp: string; // Wall-clock time for reference
  source: string;
  sequence_id: number;
  chunk_start_time: number; // Legacy field
  is_partial: boolean;
  confidence: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time: number; // Seconds from recording start
  audio_end_time: number;   // Seconds from recording start
  duration: number;          // Segment duration in seconds
}

export interface Block {
  id: string;
  type: string;
  content: string;
  color: string;
}

export interface Section {
  title: string;
  blocks: Block[];
}

export interface Summary {
  [key: string]: Section;
}

export interface ApiResponse {
  message: string;
  num_chunks: number;
  data: any[];
}

export interface SummaryResponse {
  status: string;
  summary: Summary;
  raw_summary?: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

// BlockNote-specific types
export type SummaryFormat = 'legacy' | 'markdown' | 'blocknote';

export interface BlockNoteBlock {
  id: string;
  type: string;
  props?: Record<string, any>;
  content?: any[];
  children?: BlockNoteBlock[];
}

export interface SummaryDataResponse {
  markdown?: string;
  summary_json?: BlockNoteBlock[];
  reasoning_stripped?: boolean;
  normalization_fallback?: boolean;
  // Legacy format fields
  MeetingName?: string;
  _section_order?: string[];
  [key: string]: any; // For legacy section data
}

export type MeetingSummary = Summary | SummaryDataResponse;

export type SummaryProcessStatus =
  | 'pending'
  | 'processing'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'error'
  | 'idle';

export interface ProcessTranscriptResponse {
  message: string;
  process_id: string;
}

export interface CancelSummaryResponse {
  cancelled: boolean;
  message: string;
  meeting_id: string;
}

export interface SummaryProcessResponse {
  status: SummaryProcessStatus;
  meetingName: string | null;
  meeting_id: string;
  start: string | null;
  end: string | null;
  data: unknown | null;
  error: string | null;
}

// Pagination types for optimized transcript loading
export interface MeetingMetadata {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  folder_path?: string;
  /**
   * The meeting's own moment (GLOSSARY: «дата встречи»), additive. Absent while
   * unknown — the interface omits the date instead of substituting "now".
   */
  recorded_at?: string | null;
}

export interface PaginatedTranscriptsResponse {
  transcripts: Transcript[];
  total_count: number;
  has_more: boolean;
}

/**
 * One row of the meeting's speaker panel (`api_get_meeting_speakers`).
 * Also used by the per-row speaker dropdown on the transcript lines.
 */
export interface MeetingSpeaker {
  /** Diarization label, e.g. `speaker_0`. */
  label: string;
  /** Assigned person's name; `null` while the label is unnamed. */
  speaker_name: string | null;
  /** Number of segments carrying this label. */
  segment_count: number;
  /** Start time of the first segment; `null` when unknown. */
  first_start_time: number | null;
  /** Directory names suggested while picking, most-used first. */
  top_speaker_names: string[];
}

/** One entry of the global speaker-name directory (`api_get_speaker_names`). */
export interface SpeakerNameEntry {
  id: string;
  name: string;
  meeting_count: number;
}

/** One item of the batch assignment sent by the panel's Done button. */
export interface SpeakerAssignment {
  label: string;
  /** `null` removes the assignment ("No name"). */
  name: string | null;
}

// Transcript segment data for virtualized display
export interface TranscriptSegmentData {
  id: string;
  timestamp: number; // audio_start_time in seconds
  endTime?: number; // audio_end_time in seconds
  text: string;
  confidence?: number;
  /** Diarization speaker label (`speaker_0`, `speaker_1`, …); absent when not diarized. */
  speaker?: string;
  /** Assigned person's name for this segment's label; absent while unnamed. */
  speaker_name?: string;
}
