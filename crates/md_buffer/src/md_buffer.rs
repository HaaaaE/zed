use std::{
    cmp, mem,
    future::Future,
    ops::{Deref, Range},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use markdown_wysiwyg::MarkdownSyntaxTree;
use md_text::{
    Buffer as TextBuffer, BufferId, BufferSnapshot as TextBufferSnapshot, Global, Lamport,
    LineEnding, ReplicaId, Result, Rope, ToOffset, Transaction, TransactionId,
};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub struct Buffer {
    text: TextBuffer,
    saved_version: Global,
    preview_version: Global,
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
        let preview_version = saved_version.clone();
        let cached_syntax_version = saved_version.clone();
        Self {
            text,
            saved_version,
            preview_version,
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
        let preview_version = saved_version.clone();
        let cached_syntax_version = saved_version.clone();
        Self {
            text,
            saved_version,
            preview_version,
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

    pub fn as_text_snapshot(&self) -> &TextBufferSnapshot {
        self.text.snapshot()
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

    pub fn replica_id(&self) -> ReplicaId {
        self.text.replica_id()
    }

    pub fn remote_id(&self) -> BufferId {
        self.text.remote_id()
    }

    pub fn version(&self) -> Global {
        self.text.version()
    }

    pub fn base_text(&self) -> &Rope {
        self.text.base_text()
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

    pub fn deferred_ops_len(&self) -> usize {
        self.text.deferred_ops_len()
    }

    pub fn has_deferred_ops(&self) -> bool {
        self.text.has_deferred_ops()
    }

    pub fn text(&self) -> String {
        self.text.text()
    }

    pub fn has_edits_since(&self, version: &Global) -> bool {
        self.text.has_edits_since(version)
    }

    pub fn has_unsaved_edits(&self) -> bool {
        self.text.has_edits_since(&self.saved_version)
    }

    pub fn is_dirty(&self) -> bool {
        self.has_unsaved_edits()
    }

    pub fn refresh_preview(&mut self) {
        self.preview_version = self.text.version();
    }

    pub fn preserve_preview(&self) -> bool {
        !self.text.has_edits_since(&self.preview_version)
    }

    pub fn did_save(&mut self, version: Global) {
        self.saved_version = version;
    }

    pub fn did_save_at_current_version(&mut self) {
        self.saved_version = self.text.version();
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.text.set_line_ending(line_ending);
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
        self.edit_internal(edits_iter, true)
    }

    pub fn edit_non_coalesce<I, S, T>(&mut self, edits_iter: I) -> Option<Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits_iter, false)
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

    pub fn undo_transaction(&mut self, transaction_id: TransactionId) -> bool {
        self.text.undo_transaction(transaction_id).is_some()
    }

    pub fn undo_to_transaction(&mut self, transaction_id: TransactionId) -> bool {
        !self.text.undo_to_transaction(transaction_id).is_empty()
    }

    pub fn redo(&mut self) -> Option<TransactionId> {
        self.text.redo().map(|(transaction_id, _)| transaction_id)
    }

    pub fn redo_to_transaction(&mut self, transaction_id: TransactionId) -> bool {
        !self.text.redo_to_transaction(transaction_id).is_empty()
    }

    pub fn transaction_group_interval(&self) -> Duration {
        self.text.transaction_group_interval()
    }

    pub fn set_group_interval(&mut self, group_interval: Duration) {
        self.text.set_group_interval(group_interval);
    }

    pub fn wait_for_version(&mut self, version: Global) -> impl Future<Output = Result<()>> + use<> {
        self.text.wait_for_version(version)
    }

    pub fn give_up_waiting(&mut self) {
        self.text.give_up_waiting();
    }

    fn edit_internal<I, S, T>(&mut self, edits_iter: I, coalesce_adjacent: bool) -> Option<Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        let mut edits: Vec<(Range<usize>, Arc<str>)> = Vec::new();

        for (range, new_text) in edits_iter {
            let mut range = range.start.to_offset(&self.text)..range.end.to_offset(&self.text);

            if range.start > range.end {
                mem::swap(&mut range.start, &mut range.end);
            }
            let new_text = new_text.into();
            if !new_text.is_empty() || !range.is_empty() {
                let previous_edit = edits.last_mut();
                let should_coalesce = previous_edit.as_ref().is_some_and(|(previous_range, _)| {
                    if coalesce_adjacent {
                        previous_range.end >= range.start
                    } else {
                        previous_range.end > range.start
                    }
                });

                if let Some((previous_range, previous_text)) = previous_edit
                    && should_coalesce
                {
                    previous_range.end = cmp::max(previous_range.end, range.end);
                    *previous_text = format!("{previous_text}{new_text}").into();
                } else {
                    edits.push((range, new_text));
                }
            }
        }

        if edits.is_empty() {
            return None;
        }

        let operation = self.text.edit(edits);
        Some(operation.timestamp())
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
    use std::{
        pin::Pin,
        task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    };

    const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        noop_clone_raw_waker,
        noop_wake_raw_waker,
        noop_wake_raw_waker,
        noop_drop_raw_waker,
    );

    fn noop_clone_raw_waker(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
    }

    fn noop_wake_raw_waker(_: *const ()) {}

    fn noop_drop_raw_waker(_: *const ()) {}

    fn noop_waker() -> Waker {
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)) }
    }

    fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);
        future.poll(&mut context)
    }

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
    fn local_buffer_exposes_identity_and_base_text() {
        let mut buffer = Buffer::local("alpha\r\nbeta");

        assert_eq!(buffer.replica_id(), ReplicaId::LOCAL);
        assert_eq!(buffer.as_text_snapshot().replica_id(), buffer.replica_id());
        assert_eq!(buffer.as_text_snapshot().remote_id(), buffer.remote_id());
        assert_eq!(buffer.text_snapshot().replica_id(), buffer.replica_id());
        assert_eq!(buffer.text_snapshot().remote_id(), buffer.remote_id());
        assert_eq!(buffer.base_text().to_string(), "alpha\nbeta");

        assert!(buffer.append("\ngamma").is_some());
        assert_eq!(buffer.base_text().to_string(), "alpha\nbeta");
    }

    #[test]
    fn has_edits_since_tracks_content_changes_across_undo() {
        let mut buffer = Buffer::local("hello");
        let version = buffer.version();

        assert!(!buffer.has_edits_since(&version));

        assert!(buffer.append(" world").is_some());
        assert!(buffer.has_edits_since(&version));

        assert!(buffer.undo().is_some());
        assert!(!buffer.has_edits_since(&version));
    }

    #[test]
    fn local_buffer_has_no_deferred_ops() {
        let mut buffer = Buffer::local("abc");

        assert_eq!(buffer.deferred_ops_len(), 0);
        assert!(!buffer.has_deferred_ops());

        assert!(buffer.append("d").is_some());
        assert_eq!(buffer.deferred_ops_len(), 0);
        assert!(!buffer.has_deferred_ops());
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

    #[test]
    fn undo_back_to_saved_state_clears_dirty_flag() {
        let mut buffer = Buffer::local("hello");
        buffer.did_save_at_current_version();

        assert!(buffer.append(" world").is_some());
        assert!(buffer.is_dirty());

        assert!(buffer.undo().is_some());

        assert_eq!(buffer.text(), "hello");
        assert!(!buffer.is_dirty());
        assert!(!buffer.snapshot().is_dirty());
    }

    #[test]
    fn transaction_undo_and_redo_respect_saved_state() {
        let mut buffer = Buffer::local("A");
        buffer.did_save_at_current_version();

        assert!(buffer.append("B").is_some());
        let first_transaction = buffer.last_transaction_id().unwrap();
        assert!(buffer.finalize_last_transaction().is_some());
        assert!(buffer.append("C").is_some());
        let latest_transaction = buffer.last_transaction_id().unwrap();

        assert!(buffer.undo_transaction(latest_transaction));
        assert_eq!(buffer.text(), "AB");
        assert!(buffer.is_dirty());

        assert!(buffer.undo_transaction(first_transaction));
        assert_eq!(buffer.text(), "A");
        assert!(!buffer.is_dirty());

        assert!(buffer.redo_to_transaction(latest_transaction));
        assert_eq!(buffer.text(), "ABC");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn edit_non_coalesce_normalizes_ranges_and_group_interval_round_trips() {
        let mut buffer = Buffer::local("abcd");
        let group_interval = std::time::Duration::from_millis(42);

        buffer.set_group_interval(group_interval);
        assert_eq!(buffer.transaction_group_interval(), group_interval);

        assert!(buffer.edit_non_coalesce([(3..1, "X")]).is_some());
        assert_eq!(buffer.text(), "aXd");
    }

    #[test]
    fn set_line_ending_updates_snapshot_metadata() {
        let mut buffer = Buffer::local("one\ntwo\n");

        assert_eq!(buffer.line_ending(), LineEnding::Unix);
        buffer.set_line_ending(LineEnding::Windows);

        let snapshot = buffer.snapshot();
        assert_eq!(buffer.line_ending(), LineEnding::Windows);
        assert_eq!(snapshot.line_ending(), LineEnding::Windows);
        assert_eq!(snapshot.text(), "one\ntwo\n");
    }

    #[test]
    fn refresh_preview_preserves_only_current_buffer_state() {
        let mut buffer = Buffer::local("A");

        assert!(buffer.preserve_preview());

        assert!(buffer.append("B").is_some());
        assert!(!buffer.preserve_preview());

        buffer.refresh_preview();
        assert!(buffer.preserve_preview());
        assert!(buffer.finalize_last_transaction().is_some());

        assert!(buffer.append("C").is_some());
        assert!(!buffer.preserve_preview());

        assert!(buffer.undo().is_some());
        assert!(buffer.preserve_preview());
    }

    #[test]
    fn wait_for_current_version_is_immediately_ready() {
        let mut buffer = Buffer::local("ready");
        let version = buffer.version();

        let mut future = Box::pin(buffer.wait_for_version(version));

        assert!(matches!(poll_once(future.as_mut()), Poll::Ready(Ok(()))));
    }

    #[test]
    fn wait_for_version_resolves_after_local_edit_reaches_target() {
        let mut buffer = Buffer::local("A");
        let transaction_id = buffer.start_transaction().unwrap();
        let target_timestamp = Lamport {
            replica_id: transaction_id.replica_id,
            value: transaction_id.value + 1,
        };
        let mut target_version = buffer.version();
        target_version.observe(target_timestamp);

        let mut future = Box::pin(buffer.wait_for_version(target_version.clone()));
        assert!(matches!(poll_once(future.as_mut()), Poll::Pending));

        let edit_timestamp = buffer.append("B").unwrap();
        assert_eq!(edit_timestamp, target_timestamp);
        assert_eq!(buffer.end_transaction(), Some(transaction_id));

        assert!(matches!(poll_once(future.as_mut()), Poll::Ready(Ok(()))));
    }

    #[test]
    fn give_up_waiting_fails_pending_waiters() {
        let mut buffer = Buffer::local("A");
        let mut target_version = buffer.version();
        target_version.observe(Lamport {
            replica_id: buffer.replica_id(),
            value: u32::MAX,
        });

        let mut future = Box::pin(buffer.wait_for_version(target_version));
        assert!(matches!(poll_once(future.as_mut()), Poll::Pending));

        buffer.give_up_waiting();

        assert!(matches!(poll_once(future.as_mut()), Poll::Ready(Err(_))));
    }
}

