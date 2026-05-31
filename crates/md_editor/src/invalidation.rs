use std::ops::Range;

use md_buffer::BufferEditSummary;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EditLayoutInvalidation {
    Conservative,
    LocalSourceSelection {
        edit_summary: Option<BufferEditSummary>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalSourceEditInvalidation {
    pub(crate) rows: Range<usize>,
    pub(crate) byte_delta: Option<isize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalRenderedEditInvalidation {
    pub(crate) rows: Range<usize>,
    pub(crate) byte_delta: Option<isize>,
}
