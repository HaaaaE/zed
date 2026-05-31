use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditLayoutInvalidation {
    Conservative,
    LocalSourceSelection { byte_delta: Option<isize> },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalSourceEditInvalidation {
    pub(crate) rows: Range<usize>,
    pub(crate) byte_delta: Option<isize>,
}
