use std::{
    ops::{Deref, Range},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use markdown_wysiwyg::MarkdownSyntaxTree;
use md_text::{
    Buffer as TextBuffer, BufferId, BufferSnapshot as TextBufferSnapshot, Global, Lamport,
    LineEnding, ReplicaId, Rope, ToOffset, Transaction, TransactionId,
};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub struct Buffer {
    text: TextBuffer,
    saved_version: Global,
    cached_syntax_tree: MarkdownSyntaxTree,
    cached_syntax_version: Global,
}

#[derive(Clone)]
pub struct BufferSnapshot {
    text: TextBufferSnapshot,
    syntax_tree: MarkdownSyntaxTree,
    saved_version: Global,
    has_unsaved_edits: bool,
}

impl Buffer {
    pub fn local<T: Into<String>>(base_text: T) -> Self {
        let text = TextBuffer::new(ReplicaId::LOCAL, next_buffer_id(), base_text.into());
        let cached_syntax_tree = parse_markdown(text.snapshot());
        let saved_version = text.version();
        let cached_syntax_version = saved_version.clone();
        Self {
            text,
            saved_version,
            cached_syntax_tree,
            cached_syntax_version,
        }
    }

    pub fn local_normalized(base_text_normalized: Rope, line_ending: LineEnding) -> Self {
        let text = TextBuffer::new_normalized(
            ReplicaId::LOCAL,
            next_buffer_id(),
            line_ending,
            base_text_normalized,
        );
        let cached_syntax_tree = parse_markdown(text.snapshot());
        let saved_version = text.version();
        let cached_syntax_version = saved_version.clone();
        Self {
            text,
            saved_version,
            cached_syntax_tree,
            cached_syntax_version,
        }
    }

    pub fn snapshot(&mut self) -> BufferSnapshot {
        self.refresh_syntax_tree();
        BufferSnapshot {
            text: self.text.snapshot().clone(),
            syntax_tree: self.cached_syntax_tree.clone(),
            saved_version: self.saved_version.clone(),
            has_unsaved_edits: self.has_unsaved_edits(),
        }
    }

    pub fn text_snapshot(&self) -> TextBufferSnapshot {
        self.text.snapshot().clone()
    }

    pub fn syntax_tree(&mut self) -> &MarkdownSyntaxTree {
        self.refresh_syntax_tree();
        &self.cached_syntax_tree
    }

    pub fn as_text_buffer(&self) -> &TextBuffer {
        &self.text
    }

    pub fn version(&self) -> Global {
        self.text.version()
    }

    pub fn saved_version(&self) -> &Global {
        &self.saved_version
    }

    pub fn line_ending(&self) -> LineEnding {
        self.text.line_ending()
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn text(&self) -> String {
        self.text.text()
    }

    pub fn has_unsaved_edits(&self) -> bool {
        self.text.version().changed_since(&self.saved_version)
    }

    pub fn is_dirty(&self) -> bool {
        self.has_unsaved_edits()
    }

    pub fn did_save(&mut self, version: Global) {
        self.saved_version = version;
    }

    pub fn did_save_at_current_version(&mut self) {
        self.saved_version = self.text.version();
    }

    pub fn set_text<T>(&mut self, text: T) -> Option<Lamport>
    where
        T: Into<Arc<str>>,
    {
        self.edit([(0..self.len(), text)])
    }

    pub fn append<T>(&mut self, text: T) -> Option<Lamport>
    where
        T: Into<Arc<str>>,
    {
        self.edit([(self.len()..self.len(), text)])
    }

    pub fn edit<I, S, T>(&mut self, edits_iter: I) -> Option<Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        let edits: Vec<_> = edits_iter.into_iter().collect();
        if edits.is_empty() {
            return None;
        }
        let operation = self.text.edit(edits);
        Some(operation.timestamp())
    }

    pub fn start_transaction(&mut self) -> Option<TransactionId> {
        self.text.start_transaction()
    }

    pub fn start_transaction_at(&mut self, now: Instant) -> Option<TransactionId> {
        self.text.start_transaction_at(now)
    }

    pub fn end_transaction(&mut self) -> Option<TransactionId> {
        self.end_transaction_at(Instant::now())
    }

    pub fn end_transaction_at(&mut self, now: Instant) -> Option<TransactionId> {
        self.text.end_transaction_at(now).map(|(transaction_id, _)| transaction_id)
    }

    pub fn finalize_last_transaction(&mut self) -> Option<&Transaction> {
        self.text.finalize_last_transaction()
    }

    pub fn last_transaction_id(&self) -> Option<TransactionId> {
        self.text
            .peek_undo_stack()
            .map(|entry| entry.transaction_id())
    }

    pub fn group_until_transaction(&mut self, transaction_id: TransactionId) {
        self.text.group_until_transaction(transaction_id);
    }

    pub fn forget_transaction(&mut self, transaction_id: TransactionId) -> Option<Transaction> {
        self.text.forget_transaction(transaction_id)
    }

    pub fn get_transaction(&self, transaction_id: TransactionId) -> Option<&Transaction> {
        self.text.get_transaction(transaction_id)
    }

    pub fn merge_transactions(&mut self, transaction: TransactionId, destination: TransactionId) {
        self.text.merge_transactions(transaction, destination);
    }

    pub fn undo(&mut self) -> Option<TransactionId> {
        self.text.undo().map(|(transaction_id, _)| transaction_id)
    }

    pub fn redo(&mut self) -> Option<TransactionId> {
        self.text.redo().map(|(transaction_id, _)| transaction_id)
    }

    fn refresh_syntax_tree(&mut self) {
        let current_version = self.text.version();
        if self.cached_syntax_version == current_version {
            return;
        }
        self.cached_syntax_tree = parse_markdown(self.text.snapshot());
        self.cached_syntax_version = current_version;
    }
}

impl BufferSnapshot {
    pub fn as_text_snapshot(&self) -> &TextBufferSnapshot {
        &self.text
    }

    pub fn syntax_tree(&self) -> &MarkdownSyntaxTree {
        &self.syntax_tree
    }

    pub fn text(&self) -> String {
        self.text.text()
    }

    pub fn line_ending(&self) -> LineEnding {
        self.text.line_ending()
    }

    pub fn saved_version(&self) -> &Global {
        &self.saved_version
    }

    pub fn has_unsaved_edits(&self) -> bool {
        self.has_unsaved_edits
    }

    pub fn is_dirty(&self) -> bool {
        self.has_unsaved_edits
    }
}

impl Deref for BufferSnapshot {
    type Target = TextBufferSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

fn next_buffer_id() -> BufferId {
    loop {
        let raw = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
        if let Ok(buffer_id) = BufferId::new(raw) {
            return buffer_id;
        }
    }
}

fn parse_markdown(snapshot: &TextBufferSnapshot) -> MarkdownSyntaxTree {
    MarkdownSyntaxTree::parse(&snapshot.text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_wysiwyg::MarkdownBlockKind;

    #[test]
    fn local_snapshot_parses_markdown_and_normalizes_line_endings() {
        let mut buffer = Buffer::local("# Heading\r\n\r\nBody");

        let snapshot = buffer.snapshot();

        assert_eq!(snapshot.line_ending(), LineEnding::Windows);
        assert_eq!(snapshot.text(), "# Heading\n\nBody");
        assert_eq!(snapshot.version(), snapshot.saved_version());
        assert!(!snapshot.is_dirty());
        assert_eq!(
            snapshot.syntax_tree().blocks()[0].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
    }

    #[test]
    fn edits_update_dirty_state_and_saved_version() {
        let mut buffer = Buffer::local("Paragraph");

        assert!(!buffer.is_dirty());
        assert!(buffer.append("\n# Heading").is_some());
        assert!(buffer.is_dirty());

        let snapshot = buffer.snapshot();
        assert!(snapshot.is_dirty());
        assert_eq!(
            snapshot
                .syntax_tree()
                .blocks()
                .iter()
                .filter(|block| matches!(block.kind, MarkdownBlockKind::AtxHeading { .. }))
                .count(),
            1
        );

        let saved_version = buffer.version();
        buffer.did_save(saved_version.clone());

        assert_eq!(buffer.saved_version(), &saved_version);
        assert!(!buffer.is_dirty());
        assert!(!buffer.snapshot().is_dirty());
    }

    #[test]
    fn undo_and_redo_refresh_markdown_snapshot() {
        let mut buffer = Buffer::local("# One\n");

        assert!(buffer.append("\n## Two\n").is_some());
        assert_eq!(
            buffer
                .snapshot()
                .syntax_tree()
                .blocks()
                .iter()
                .filter(|block| matches!(block.kind, MarkdownBlockKind::AtxHeading { .. }))
                .count(),
            2
        );

        assert!(buffer.undo().is_some());
        assert_eq!(
            buffer
                .snapshot()
                .syntax_tree()
                .blocks()
                .iter()
                .filter(|block| matches!(block.kind, MarkdownBlockKind::AtxHeading { .. }))
                .count(),
            1
        );

        assert!(buffer.redo().is_some());
        assert_eq!(
            buffer
                .snapshot()
                .syntax_tree()
                .blocks()
                .iter()
                .filter(|block| matches!(block.kind, MarkdownBlockKind::AtxHeading { .. }))
                .count(),
            2
        );
    }
}

