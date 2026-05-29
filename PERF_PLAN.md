# Perf Framework and Session Plan

## Goal and Boundaries

- Keep the public entrypoint unchanged: continue using `#[perf]` and `cargo perf-test -p md_editor`.
- Refactor perf from isolated cases with a single total duration into structured session cases with ordered segment timelines.
- Preserve the existing capabilities: importance, weight, iterations, filtering, Markdown reporting, JSON output, and run comparison.
- This migration is intentionally breaking for old total-only perf output and old run JSON.
- Do not measure real process startup, real app binary startup, or file IO in this pass. Only measure paths that fit the current `#[perf]` test flow.
- Do not use historical `.perf-runs` data to drive this migration.

## Final Architecture

- Split `tooling/perf` into clearer responsibilities:
  - `protocol`: protocol constants, metadata parsing, and sample output parsing.
  - `model`: structured result types for cases, samples, totals, segments, and summaries.
  - `runner`: perf test discovery, metadata execution, warmup, sampling, and importance filtering.
  - `report`: Markdown output for total timings and segment timelines.
  - `compare`: run comparison for total timings and matching segment occurrences.
  - `main`: CLI argument parsing and top-level dispatch only.
- Keep the `perf` crate API needed by `util_macros` stable, especially `consts`, `Importance`, and metadata constants.
- Extend the protocol:
  - `MD_PERF_SELF_TIMED_NS <ns>` remains the required total duration line for every sample.
  - `MD_PERF_SEGMENT_NS <name> <ns>` is required and may appear multiple times.
  - Segment names must use `[a-z0-9_]+`.
  - Repeated segment names are allowed and must be preserved in output order.
- Update the data model:
  - Each sample stores one `total` and an ordered `segments` list.
  - Each case summarizes total mean/stddev.
  - Segment summaries are grouped by timeline occurrence, not just by name.
  - Total-only perf cases are invalid.
- Update reports:
  - Keep the main total table with iter/sec, mean, standard deviation, iterations, and importance.
  - Add a segment timeline table: `case | # | segment | mean ms | sd ms | % total`.
  - Keep the existing importance/weight total comparison and add segment occurrence deltas where both runs have matching segments.

## Md Editor Perf Sessions

- Replace the existing independent `md_editor` perf cases with two top-level session cases:
  - `small_document_session`: about 5 KiB Markdown, `#[perf(important, iterations = 16)]`.
  - `large_document_session`: about 300 KiB Markdown, `#[perf(important, iterations = 8)]`.
- Keep fixture generation in memory. Do not include disk IO.
- Each iteration runs the full session so cache state, mode switches, scroll history, and edit invalidation affect later steps naturally.
- Each session records this ordered workflow:
  - `fixture_prepare`
  - `context_create`
  - `window_create`
  - `source_editor_create`
  - `source_first_draw`
  - `source_cached_redraw`
  - `source_scroll_cold_prepare`
  - `source_scroll_cold`
  - `source_scroll_cached_region_prepare`
  - `source_scroll_cached_region`
  - `source_edit_equal_length`
  - `source_edit_length_change`
  - `source_cached_redraw_after_edit`
  - `switch_to_rendered`
  - `rendered_first_draw`
  - `rendered_cached_redraw`
  - `rendered_scroll_cold_prepare`
  - `rendered_scroll_cold`
  - `rendered_scroll_cached_region_prepare`
  - `rendered_scroll_cached_region`
  - `rendered_resize`
  - `rendered_cached_redraw`
  - `rendered_scroll_cold_prepare`
  - `rendered_scroll_cold`
  - `rendered_scroll_cached_region_prepare`
  - `rendered_scroll_cached_region`
  - `rendered_resize`
  - `switch_to_source`
  - `source_scroll_cached_region_after_switch_prepare`
  - `source_scroll_cached_region_after_switch`
  - `source_edit_equal_length_after_switch`
  - `source_edit_length_change_after_switch`
  - `switch_to_rendered_again`
  - `rendered_cached_redraw_after_second_switch`
- Scroll prepare segments are explicit so scroll hot-path segments do not hide
  cache clearing, region prewarming, or re-anchoring setup.
- Repeated segment names are intentional. The report must distinguish occurrences by timeline index.

## Implementation Steps

- Refactor `tooling/perf` first:
  - Extract protocol parsing and add a parser for total plus segment output.
  - Replace duration-only sample collection with structured sample collection.
  - Update `Output` and related JSON types to store sample timelines and summaries.
  - Update runner sampling to collect warmup plus eight structured samples.
  - Update Markdown reporting to include both total and segment timeline tables.
  - Update comparison to preserve current total comparison and add segment occurrence comparison.
- Migrate `md_editor` perf tests:
  - Add a session helper that reads `MD_PERF_ITER`, times named segments, accumulates totals, and emits protocol lines.
  - Reuse the existing draw, scroll, edit, resize, and mode-switch helpers where possible.
  - Remove or disable the old isolated `#[perf]` cases to avoid duplicate reporting.
  - Keep correctness assertions for mode, non-empty text, valid row counts, and edits actually changing the buffer.
- Breaking cleanup:
  - Confirm the `#[perf]` macro does not require user-side callsite changes.
  - Remove total-only parsing/reporting behavior.
  - Keep JSON reading strict for segmented output fields.

## Failure Behavior

- Missing `MD_PERF_SELF_TIMED_NS`: profile failure.
- Missing `MD_PERF_SEGMENT_NS`: profile failure.
- More than one total line in a sample: profile failure.
- Non-integer total or segment duration: profile failure.
- Illegal segment name: profile failure.
- Repeated segment name in one sample: valid, preserve occurrence order.
- Different segment timelines across samples for the same case: profile failure, because occurrence comparison would be ambiguous.
- Metadata parse failure, version mismatch, and importance filtering keep the current failure/skip semantics.

## Test Plan

- Do not run `cargo perf-test` unless explicitly allowed later.
- Add normal unit tests for `tooling/perf`:
  - Parse total plus segments.
  - Preserve repeated segment occurrences.
  - Reject missing total, multiple totals, invalid segment names, and invalid durations.
  - Reject inconsistent segment timelines across samples.
  - Render reports with both total and segment tables.
  - Compare total timings and matching segment occurrences.
- Verification commands:
- `cargo test -p perf`
- `cargo test -p util_macros`
- `cargo test -p md_editor -- --list`
- `cargo test -p md_editor small_document_session -- --nocapture` as a smoke test, not as performance data.
- `cargo test -p md_editor large_document_session -- --nocapture` is optional; skip it if too slow.

## Progress Log

- 2026-05-30: Reworked `tooling/perf` around structured samples with mandatory segment timelines, added parser/report/compare coverage, and collapsed `md_editor` perf cases into `small_document_session` and `large_document_session`.
- Verified with `cargo test -p perf` and `cargo test -p md_editor --profile release-fast --lib --no-run --config 'target."cfg(true)".rustflags=["--cfg","perf_enabled"]'`.

## Acceptance Criteria

- `cargo perf-test -p md_editor` and `#[perf]` usage remain unchanged.
- The runner rejects total-only cases and supports segmented session cases.
- `md_editor` top-level perf cases are reduced to `small_document_session` and `large_document_session`.
- Reports show session total timings and ordered segment timings with occurrence indexes.
- Repeated segments are not merged or overwritten.
- Existing hot-path coverage remains present inside the session workflows.
