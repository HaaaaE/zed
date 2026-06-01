use std::{collections::HashMap, ops::Range, sync::OnceLock};

use tree_sitter::Node;

use super::{
    MarkdownBlock, MarkdownBlockKind, MarkdownInlineKind, MarkdownInlineSpan, MarkdownInlineTree,
    MarkdownProjectionReplacement, MarkdownSyntaxTree, ProjectionMarkerDependency,
    source::{
        old_range_for_clean_new_range, range_contains, ranges_overlap, ranges_touch,
        shift_clean_old_range_to_new,
    },
    structure::MarkdownStructure,
};

impl MarkdownSyntaxTree {
    pub fn inline_spans(&self) -> &[MarkdownInlineSpan] {
        &self.data.inline_spans
    }

    pub fn inline_spans_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownInlineSpan> {
        let start = range.start;
        let end = range.end;
        let start_index = self.partition_inline_spans_by_prefix_end(start);
        self.data.inline_spans[start_index..]
            .iter()
            .take_while(move |span| span.source_range.start < end)
            .filter(move |span| span.source_range.start < end && span.source_range.end > start)
    }

    fn partition_inline_spans_by_prefix_end(&self, offset: usize) -> usize {
        self.data
            .inline_span_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }
}

pub(super) fn collect_structure_inline_spans(
    source: &str,
    structure: &MarkdownStructure,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    for inline_tree in structure.inline_trees() {
        spans.extend(collect_inline_spans_for_inline_tree(source, inline_tree));
    }
    spans.sort_by_key(|span| (span.source_range.start, span.source_range.end));
    spans
}

pub(super) fn collect_incremental_inline_spans(
    source: &str,
    previous: &MarkdownSyntaxTree,
    structure: &MarkdownStructure,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let previous_inline_parent_ranges = previous
        .parser_state
        .inline_trees()
        .iter()
        .map(|inline_tree| (inline_tree.parent_range.clone(), ()))
        .collect::<HashMap<_, _>>();
    for inline_tree in structure.inline_trees() {
        let new_parent_range = &inline_tree.parent_range;
        if ranges_touch(new_parent_range, new_range) {
            spans.extend(collect_inline_spans_for_inline_tree(source, inline_tree));
            continue;
        }

        let old_parent_range =
            old_range_for_clean_new_range(new_parent_range, new_range, old_range);
        if ranges_touch(&old_parent_range, old_range) {
            spans.extend(collect_inline_spans_for_inline_tree(source, inline_tree));
            continue;
        }

        if !previous_inline_parent_ranges.contains_key(&old_parent_range) {
            spans.extend(collect_inline_spans_for_inline_tree(source, inline_tree));
            continue;
        }

        let first_span_index = previous
            .data
            .inline_spans
            .partition_point(|span| span.source_range.start < old_parent_range.start);
        spans.extend(
            previous
                .data
                .inline_spans
                .get(first_span_index..)
                .unwrap_or_default()
                .iter()
                .take_while(|span| span.source_range.start < old_parent_range.end)
                .filter(|span| range_contains(&old_parent_range, &span.source_range))
                .cloned()
                .map(|span| shift_inline_span_after_edit(span, old_range, new_range)),
        );
    }
    spans.sort_by_key(|span| (span.source_range.start, span.source_range.end));
    spans
}

pub(super) fn collect_inline_spans_for_inline_tree(
    source: &str,
    inline_tree: &MarkdownInlineTree,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    collect_inline_span_nodes(source, inline_tree.tree().root_node(), &mut spans);
    collect_soft_break_spans(source, inline_tree.parent_range.clone(), &mut spans);
    spans
}

fn collect_soft_break_spans(
    source: &str,
    parent_range: Range<usize>,
    spans: &mut Vec<MarkdownInlineSpan>,
) {
    let mut cursor = parent_range.start;
    while cursor < parent_range.end {
        let Some(relative_newline) = source[cursor..parent_range.end].find('\n') else {
            break;
        };
        let newline = cursor + relative_newline;
        let next = newline + 1;
        if next >= parent_range.end {
            break;
        }

        let has_carriage_return =
            newline > parent_range.start && source.as_bytes()[newline - 1] == b'\r';
        let break_start = if has_carriage_return {
            newline - 1
        } else {
            newline
        };
        let break_range = break_start..next;
        let is_hard_break = spans.iter().any(|span| {
            span.kind == MarkdownInlineKind::HardBreak
                && ranges_overlap(&span.source_range, &break_range)
        });
        if !is_hard_break {
            spans.push(MarkdownInlineSpan {
                kind: MarkdownInlineKind::SoftBreak,
                source_range: break_range.clone(),
                content_ranges: vec![break_range],
                marker_ranges: Vec::new(),
                url: None,
                tagfilter_disallowed: false,
            });
        }

        cursor = next;
    }
}

pub(super) fn inline_span_prefix_maximum_ends(inline_spans: &[MarkdownInlineSpan]) -> Vec<usize> {
    let mut maximum_end = 0;
    inline_spans
        .iter()
        .map(|span| {
            maximum_end = maximum_end.max(span.source_range.end);
            maximum_end
        })
        .collect()
}

pub(super) fn collect_projection_replacements_for_blocks(
    source: &str,
    blocks: &[MarkdownBlock],
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = Vec::new();
    for block in blocks {
        if let Some(replacement) = task_list_marker_replacement_from_block(source, block) {
            replacements.push(replacement);
        }
    }
    replacements
}

pub(super) fn collect_structure_projection_replacements(
    source: &str,
    structure: &MarkdownStructure,
    blocks: &[MarkdownBlock],
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = collect_projection_replacements_for_blocks(source, blocks);
    for inline_tree in structure.inline_trees() {
        replacements.extend(collect_projection_replacements_for_inline_tree(
            source,
            inline_tree,
        ));
    }
    replacements.sort_by_key(|replacement| {
        (
            replacement.source_range.start,
            replacement.source_range.end,
            replacement.owner_source_range.start,
            replacement.owner_source_range.end,
        )
    });
    replacements
}

pub(super) fn collect_incremental_projection_replacements(
    source: &str,
    previous: &MarkdownSyntaxTree,
    structure: &MarkdownStructure,
    blocks: &[MarkdownBlock],
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = collect_projection_replacements_for_blocks(source, blocks);
    let previous_inline_parent_ranges = previous
        .parser_state
        .inline_trees()
        .iter()
        .map(|inline_tree| (inline_tree.parent_range.clone(), ()))
        .collect::<HashMap<_, _>>();
    for inline_tree in structure.inline_trees() {
        let new_parent_range = &inline_tree.parent_range;
        if ranges_touch(new_parent_range, new_range) {
            replacements.extend(collect_projection_replacements_for_inline_tree(
                source,
                inline_tree,
            ));
            continue;
        }

        let old_parent_range =
            old_range_for_clean_new_range(new_parent_range, new_range, old_range);
        if ranges_touch(&old_parent_range, old_range) {
            replacements.extend(collect_projection_replacements_for_inline_tree(
                source,
                inline_tree,
            ));
            continue;
        }

        if !previous_inline_parent_ranges.contains_key(&old_parent_range) {
            replacements.extend(collect_projection_replacements_for_inline_tree(
                source,
                inline_tree,
            ));
            continue;
        }

        let first_replacement_index = previous
            .data
            .projection_replacements
            .partition_point(|replacement| replacement.source_range.start < old_parent_range.start);
        replacements.extend(
            previous
                .data
                .projection_replacements
                .get(first_replacement_index..)
                .unwrap_or_default()
                .iter()
                .take_while(|replacement| replacement.source_range.start < old_parent_range.end)
                .filter(|replacement| {
                    range_contains(&old_parent_range, &replacement.owner_source_range)
                })
                .cloned()
                .map(|replacement| {
                    shift_projection_replacement_after_edit(replacement, old_range, new_range)
                }),
        );
    }
    replacements.sort_by_key(|replacement| {
        (
            replacement.source_range.start,
            replacement.source_range.end,
            replacement.owner_source_range.start,
            replacement.owner_source_range.end,
        )
    });
    replacements
}

pub(super) fn collect_projection_replacements_for_inline_tree(
    source: &str,
    inline_tree: &MarkdownInlineTree,
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = Vec::new();
    collect_projection_replacement_nodes(source, inline_tree.tree().root_node(), &mut replacements);
    replacements
}

pub(super) fn projection_replacement_prefix_maximum_ends(
    replacements: &[MarkdownProjectionReplacement],
) -> Vec<usize> {
    let mut maximum_end = 0;
    replacements
        .iter()
        .map(|replacement| {
            maximum_end = maximum_end.max(replacement.source_range.end);
            maximum_end
        })
        .collect()
}

pub(super) fn projection_marker_dependencies(
    blocks: &[MarkdownBlock],
    inline_spans: &[MarkdownInlineSpan],
    replacements: &[MarkdownProjectionReplacement],
) -> Vec<ProjectionMarkerDependency> {
    let mut dependencies = Vec::new();
    for block in blocks {
        dependencies.extend(block.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: block.source_range.clone(),
            }
        }));
    }
    for span in inline_spans {
        if matches!(
            span.kind,
            MarkdownInlineKind::SoftBreak | MarkdownInlineKind::HardBreak
        ) {
            dependencies.push(ProjectionMarkerDependency {
                marker_range: span.source_range.clone(),
                owner_source_range: span.source_range.clone(),
            });
        }
        dependencies.extend(span.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: span.source_range.clone(),
            }
        }));
    }
    dependencies.extend(
        replacements
            .iter()
            .map(|replacement| ProjectionMarkerDependency {
                marker_range: replacement.source_range.clone(),
                owner_source_range: replacement.owner_source_range.clone(),
            }),
    );
    dependencies.sort_by_key(|dependency| {
        (
            dependency.marker_range.start,
            dependency.marker_range.end,
            dependency.owner_source_range.start,
            dependency.owner_source_range.end,
        )
    });
    dependencies
}

pub(super) fn collect_incremental_projection_marker_dependencies(
    previous: &MarkdownSyntaxTree,
    blocks: &[MarkdownBlock],
    inline_spans: &[MarkdownInlineSpan],
    replacements: &[MarkdownProjectionReplacement],
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> Vec<ProjectionMarkerDependency> {
    let mut dependencies = previous
        .data
        .projection_marker_dependencies
        .iter()
        .filter(|dependency| {
            !ranges_touch(&dependency.marker_range, old_range)
                && !ranges_touch(&dependency.owner_source_range, old_range)
        })
        .cloned()
        .map(|dependency| {
            shift_projection_marker_dependency_after_edit(dependency, old_range, new_range)
        })
        .collect::<Vec<_>>();

    for block in blocks {
        if !ranges_touch(&block.source_range, new_range) {
            continue;
        }

        dependencies.extend(block.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: block.source_range.clone(),
            }
        }));
    }

    for span in inline_spans {
        if !ranges_touch(&span.source_range, new_range) {
            continue;
        }

        if matches!(
            span.kind,
            MarkdownInlineKind::SoftBreak | MarkdownInlineKind::HardBreak
        ) {
            dependencies.push(ProjectionMarkerDependency {
                marker_range: span.source_range.clone(),
                owner_source_range: span.source_range.clone(),
            });
        }
        dependencies.extend(span.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: span.source_range.clone(),
            }
        }));
    }

    for replacement in replacements {
        if !ranges_touch(&replacement.owner_source_range, new_range)
            && !ranges_touch(&replacement.source_range, new_range)
        {
            continue;
        }

        dependencies.push(ProjectionMarkerDependency {
            marker_range: replacement.source_range.clone(),
            owner_source_range: replacement.owner_source_range.clone(),
        });
    }

    dependencies.sort_by_key(|dependency| {
        (
            dependency.marker_range.start,
            dependency.marker_range.end,
            dependency.owner_source_range.start,
            dependency.owner_source_range.end,
        )
    });
    dependencies
}

pub(super) fn projection_marker_prefix_maximum_ends(
    dependencies: &[ProjectionMarkerDependency],
) -> Vec<usize> {
    let mut maximum_end = 0;
    dependencies
        .iter()
        .map(|dependency| {
            maximum_end = maximum_end.max(dependency.marker_range.end);
            maximum_end
        })
        .collect()
}

fn shift_inline_span_after_edit(
    mut span: MarkdownInlineSpan,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> MarkdownInlineSpan {
    span.source_range = shift_clean_old_range_to_new(span.source_range, old_range, new_range);
    span.content_ranges = span
        .content_ranges
        .into_iter()
        .map(|range| shift_clean_old_range_to_new(range, old_range, new_range))
        .collect();
    span.marker_ranges = span
        .marker_ranges
        .into_iter()
        .map(|range| shift_clean_old_range_to_new(range, old_range, new_range))
        .collect();
    span
}

fn shift_projection_replacement_after_edit(
    mut replacement: MarkdownProjectionReplacement,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> MarkdownProjectionReplacement {
    replacement.source_range =
        shift_clean_old_range_to_new(replacement.source_range, old_range, new_range);
    replacement.owner_source_range =
        shift_clean_old_range_to_new(replacement.owner_source_range, old_range, new_range);
    replacement
}

fn shift_projection_marker_dependency_after_edit(
    mut dependency: ProjectionMarkerDependency,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> ProjectionMarkerDependency {
    dependency.marker_range =
        shift_clean_old_range_to_new(dependency.marker_range, old_range, new_range);
    dependency.owner_source_range =
        shift_clean_old_range_to_new(dependency.owner_source_range, old_range, new_range);
    dependency
}

fn collect_projection_replacement_nodes(
    source: &str,
    node: Node<'_>,
    replacements: &mut Vec<MarkdownProjectionReplacement>,
) {
    if let Some(replacement) = projection_replacement_from_node(source, node) {
        replacements.push(replacement);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_projection_replacement_nodes(source, child, replacements);
    }
}

fn projection_replacement_from_node(
    source: &str,
    node: Node<'_>,
) -> Option<MarkdownProjectionReplacement> {
    let source_range = node.byte_range();
    let display_text = match node.kind() {
        "backslash_escape" => source
            .get(source_range.start + 1..source_range.end)?
            .to_string(),
        "entity_reference" | "numeric_character_reference" => {
            decode_markdown_entity(source.get(source_range.clone())?)?
        }
        "task_list_marker_checked" => "\u{2611}".to_string(),
        "task_list_marker_unchecked" => "\u{2610}".to_string(),
        _ => return None,
    };

    Some(MarkdownProjectionReplacement {
        source_range: source_range.clone(),
        owner_source_range: source_range,
        display_text,
    })
}

fn task_list_marker_replacement_from_block(
    source: &str,
    block: &MarkdownBlock,
) -> Option<MarkdownProjectionReplacement> {
    let MarkdownBlockKind::TaskListItem { checked } = block.kind else {
        return None;
    };

    let source_range_start = block
        .marker_ranges
        .first()
        .map_or(block.source_range.start, |range| range.end);
    let source_range_end = source_range_start.checked_add(3)?;
    if source_range_end > block.source_range.end {
        return None;
    }

    let bytes = source.as_bytes();
    if bytes[source_range_start] != b'[' || bytes[source_range_start + 2] != b']' {
        return None;
    }

    let source_range = source_range_start..source_range_end;
    Some(MarkdownProjectionReplacement {
        source_range: source_range.clone(),
        owner_source_range: source_range,
        display_text: if checked {
            "\u{2611}".to_string()
        } else {
            "\u{2610}".to_string()
        },
    })
}

fn decode_markdown_entity(entity: &str) -> Option<String> {
    let entity_body = entity.strip_prefix('&')?.strip_suffix(';')?;
    if let Some(decimal) = entity_body.strip_prefix('#') {
        let codepoint = if let Some(hex) = decimal
            .strip_prefix('x')
            .or_else(|| decimal.strip_prefix('X'))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            decimal.parse::<u32>().ok()?
        };
        return char::from_u32(codepoint).map(|character| character.to_string());
    }

    decode_html5_named_character_reference(entity).map(str::to_string)
}

fn decode_html5_named_character_reference(entity: &str) -> Option<&'static str> {
    static NAMED_ENTITIES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    NAMED_ENTITIES
        .get_or_init(|| {
            entities::ENTITIES
                .iter()
                .filter(|entry| entry.entity.ends_with(';'))
                .map(|entry| (entry.entity, entry.characters))
                .collect()
        })
        .get(entity)
        .copied()
}

fn collect_inline_span_nodes(source: &str, node: Node<'_>, spans: &mut Vec<MarkdownInlineSpan>) {
    if let Some(span) = inline_span_from_node(source, node) {
        spans.push(span);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_inline_span_nodes(source, child, spans);
    }
}

fn inline_span_from_node(source: &str, node: Node<'_>) -> Option<MarkdownInlineSpan> {
    let kind = match node.kind() {
        "emphasis" => MarkdownInlineKind::Emphasis,
        "strong_emphasis" => MarkdownInlineKind::Strong,
        "code_span" => MarkdownInlineKind::InlineCode,
        "inline_link"
        | "full_reference_link"
        | "collapsed_reference_link"
        | "shortcut_link"
        | "uri_autolink"
        | "email_autolink" => MarkdownInlineKind::Link,
        "strikethrough" => MarkdownInlineKind::Strikethrough,
        "backslash_escape" => MarkdownInlineKind::Escape,
        "entity_reference" | "numeric_character_reference" => MarkdownInlineKind::Entity,
        "hard_line_break" => MarkdownInlineKind::HardBreak,
        "soft_line_break" => MarkdownInlineKind::SoftBreak,
        "html_tag" => MarkdownInlineKind::InlineHtml,
        "image" => MarkdownInlineKind::Image,
        "latex_block" => MarkdownInlineKind::InlineMath,
        _ => return None,
    };

    let source_range = node.byte_range();
    let marker_ranges = inline_marker_ranges(source, node);
    let content_ranges = inline_content_ranges(source_range.clone(), &marker_ranges);
    let url = match kind {
        MarkdownInlineKind::Image | MarkdownInlineKind::Link => extract_link_url(source, &node),
        _ => None,
    };
    let tagfilter_disallowed = kind == MarkdownInlineKind::InlineHtml
        && raw_html_tagfilter_disallowed(source, source_range.clone());

    Some(MarkdownInlineSpan {
        kind,
        source_range,
        content_ranges,
        marker_ranges,
        url,
        tagfilter_disallowed,
    })
}

pub(crate) fn raw_html_tagfilter_disallowed(source: &str, source_range: Range<usize>) -> bool {
    const DISALLOWED_TAGS: [&str; 9] = [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];

    let Some(text) = source.get(source_range) else {
        return false;
    };
    let bytes = text.as_bytes();
    let mut cursor = 0;

    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'<') {
        return false;
    }
    cursor += 1;
    if bytes.get(cursor) == Some(&b'/') {
        cursor += 1;
    }

    let tag_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_alphanumeric() {
        cursor += 1;
    }
    if cursor == tag_start {
        return false;
    }

    let tag_name = &text[tag_start..cursor];
    if !DISALLOWED_TAGS
        .iter()
        .any(|tag| tag_name.eq_ignore_ascii_case(tag))
    {
        return false;
    }

    matches!(
        bytes.get(cursor).copied(),
        None | Some(b'>') | Some(b'/') | Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
    )
}

fn inline_marker_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    if node.kind() == "latex_block" {
        return latex_block_marker_ranges(source, node);
    }
    if node.kind() == "html_tag" {
        return Vec::new();
    }

    let mut marker_ranges = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "latex_span_delimiter" => marker_ranges.push(child.byte_range()),
            "emphasis_delimiter"
            | "code_span_delimiter"
            | "link_destination"
            | "link_label"
            | "link_title" => marker_ranges.push(child.byte_range()),
            _ if !child.is_named() => marker_ranges.push(child.byte_range()),
            _ => {}
        }
    }
    marker_ranges.sort_by_key(|range| (range.start, range.end));
    marker_ranges
}

fn latex_block_marker_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    let source_range = node.byte_range();
    let delimiter_len = if source
        .get(source_range.clone())
        .is_some_and(|text| text.starts_with("$$") && text.ends_with("$$"))
    {
        2
    } else {
        1
    };

    let start = source_range.start;
    let end = source_range.end;
    let content_start = start.saturating_add(delimiter_len).min(end);
    let content_end = end.saturating_sub(delimiter_len).max(content_start);

    let mut marker_ranges = Vec::new();
    if start < content_start {
        marker_ranges.push(start..content_start);
    }
    if content_end < end {
        marker_ranges.push(content_end..end);
    }
    marker_ranges
}

fn extract_link_url(source: &str, node: &Node<'_>) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "link_destination" {
            let range = child.byte_range();
            return Some(source[range].to_string());
        }
    }
    None
}

fn inline_content_ranges(
    source_range: Range<usize>,
    marker_ranges: &[Range<usize>],
) -> Vec<Range<usize>> {
    let mut content_ranges = Vec::new();
    let mut start = source_range.start;
    for marker_range in marker_ranges {
        if start < marker_range.start {
            content_ranges.push(start..marker_range.start);
        }
        start = start.max(marker_range.end);
    }
    if start < source_range.end {
        content_ranges.push(start..source_range.end);
    }
    content_ranges
}
