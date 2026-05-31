//! A MdList element that can be used to render a large number of differently sized elements
//! efficiently. Clients of this API need to ensure that elements outside of the scrolled
//! area do not change their height for this element to function correctly. If your elements
//! do change height, notify the MdList element via [`MdListState::splice`] or [`MdListState::reset`].
//! In order to minimize re-renders, this element's state is stored intrusively
//! on your own views, so that your code can coordinate directly with the MdList element's cached state.
//!
//! If all of your elements are the same height, see `gpui::UniformList` for a simpler API.

#![allow(dead_code)]

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Edges, Element, ElementId,
    EntityId, FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, LayoutId, Overflow, Pixels, Point, Refineable as _, ScrollDelta, ScrollWheelEvent,
    Size, Style, StyleRefinement, Styled, Window, point, px, size,
};
use md_sum_tree::{Bias, Dimensions, SumTree};
use std::collections::VecDeque;
use std::{cell::RefCell, ops::Range, rc::Rc};

mod state;

type RenderItemFn = dyn FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static;

/// Construct a new MdList element
pub fn md_list(
    state: MdListState,
    render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> MdList {
    MdList {
        state,
        render_item: Box::new(render_item),
        style: StyleRefinement::default(),
        sizing_behavior: ListSizingBehavior::default(),
    }
}

/// A MdList element
pub struct MdList {
    state: MdListState,
    render_item: Box<RenderItemFn>,
    style: StyleRefinement,
    sizing_behavior: ListSizingBehavior,
}

impl MdList {
    /// Set the sizing behavior for the MdList.
    pub fn with_sizing_behavior(mut self, behavior: ListSizingBehavior) -> Self {
        self.sizing_behavior = behavior;
        self
    }
}

/// The MdList state that views must hold on behalf of the MdList element.
#[derive(Clone)]
pub struct MdListState(Rc<RefCell<StateInner>>);

impl std::fmt::Debug for MdListState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MdListState")
    }
}

#[cfg(any(test, perf_enabled))]
thread_local! {
    static MD_LIST_STATE_STATS: std::cell::Cell<MdListStateStats> =
        const { std::cell::Cell::new(MdListStateStats {
            full_remeasures: 0,
            item_remeasure_calls: 0,
            remeasured_items: 0,
        }) };
}

#[cfg(any(test, perf_enabled))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MdListStateStats {
    pub full_remeasures: usize,
    pub item_remeasure_calls: usize,
    pub remeasured_items: usize,
}

#[cfg(any(test, perf_enabled))]
fn update_md_list_state_stats(update: impl FnOnce(&mut MdListStateStats)) {
    MD_LIST_STATE_STATS.with(|stats| {
        let mut value = stats.get();
        update(&mut value);
        stats.set(value);
    });
}

#[cfg(any(test, perf_enabled))]
fn record_full_remeasure() {
    update_md_list_state_stats(|stats| stats.full_remeasures += 1);
}

#[cfg(not(any(test, perf_enabled)))]
fn record_full_remeasure() {}

#[cfg(any(test, perf_enabled))]
fn record_item_remeasure(range: Range<usize>) {
    update_md_list_state_stats(|stats| {
        stats.item_remeasure_calls += 1;
        stats.remeasured_items += range.len();
    });
}

#[cfg(not(any(test, perf_enabled)))]
fn record_item_remeasure(_: Range<usize>) {}

struct StateInner {
    last_layout_bounds: Option<Bounds<Pixels>>,
    last_padding: Option<Edges<Pixels>>,
    items: SumTree<ListItem>,
    logical_scroll_top: Option<ListOffset>,
    alignment: ListAlignment,
    overdraw: Pixels,
    reset: bool,
    #[allow(clippy::type_complexity)]
    scroll_handler: Option<Box<dyn FnMut(&ListScrollEvent, &mut Window, &mut App)>>,
    scrollbar_drag_start_height: Option<Pixels>,
    measuring_behavior: ListMeasuringBehavior,
    pending_scroll: Option<PendingScrollFraction>,
    follow_state: FollowState,
    default_size_hint: Option<Size<Pixels>>,
}

/// Keeps track of a fractional scroll position within an item for restoration
/// after remeasurement.
struct PendingScrollFraction {
    /// The index of the item to scroll within.
    item_ix: usize,
    /// Fractional offset (0.0 to 1.0) within the item's height.
    fraction: f32,
}

/// Controls whether the MdList automatically follows new content at the end.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FollowMode {
    /// Normal scrolling — no automatic following.
    #[default]
    Normal,
    /// The MdList should auto-scroll along with the tail, when scrolled to bottom.
    Tail,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FollowState {
    #[default]
    Normal,
    Tail {
        is_following: bool,
    },
}

impl FollowState {
    fn is_following(&self) -> bool {
        matches!(self, FollowState::Tail { is_following: true })
    }

    fn has_stopped_following(&self) -> bool {
        matches!(
            self,
            FollowState::Tail {
                is_following: false
            }
        )
    }

    fn start_following(&mut self) {
        if let FollowState::Tail {
            is_following: false,
        } = self
        {
            *self = FollowState::Tail { is_following: true };
        }
    }

    fn stop_following(&mut self) {
        if let FollowState::Tail { is_following: true } = self {
            *self = FollowState::Tail {
                is_following: false,
            };
        }
    }
}

/// Whether the MdList is scrolling from top to bottom or bottom to top.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListAlignment {
    /// The MdList is scrolling from top to bottom, like most lists.
    Top,
    /// The MdList is scrolling from bottom to top, like a chat log.
    Bottom,
}

/// A scroll event that has been converted to be in terms of the MdList's items.
pub struct ListScrollEvent {
    /// The range of items currently visible in the MdList, after applying the scroll event.
    pub visible_range: Range<usize>,

    /// The number of items that are currently visible in the MdList, after applying the scroll event.
    pub count: usize,

    /// Whether the MdList has been scrolled.
    pub is_scrolled: bool,

    /// Whether the MdList is currently in follow-tail mode (auto-scrolling to end).
    pub is_following_tail: bool,
}

/// The sizing behavior to apply during layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListSizingBehavior {
    /// The MdList should calculate its size based on the size of its items.
    Infer,
    /// The MdList should not calculate a fixed size.
    #[default]
    Auto,
}

/// The measuring behavior to apply during layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListMeasuringBehavior {
    /// Measure all items in the MdList.
    /// Note: This can be expensive for the first frame in a large MdList.
    Measure(bool),
    /// Only measure visible items
    #[default]
    Visible,
}

impl ListMeasuringBehavior {
    fn reset(&mut self) {
        match self {
            ListMeasuringBehavior::Measure(has_measured) => *has_measured = false,
            ListMeasuringBehavior::Visible => {}
        }
    }
}

/// The horizontal sizing behavior to apply during layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListHorizontalSizingBehavior {
    /// MdList items' width can never exceed the width of the MdList.
    #[default]
    FitList,
    /// MdList items' width may go over the width of the MdList, if any item is wider.
    Unconstrained,
}

struct LayoutItemsResponse {
    max_item_width: Pixels,
    scroll_top: ListOffset,
    item_layouts: VecDeque<ItemLayout>,
}

struct ItemLayout {
    index: usize,
    element: AnyElement,
    size: Size<Pixels>,
}

/// Frame state used by the [MdList] element after layout.
pub struct ListPrepaintState {
    hitbox: Hitbox,
    layout: LayoutItemsResponse,
}

#[derive(Clone)]
enum ListItem {
    Unmeasured {
        size_hint: Option<Size<Pixels>>,
        focus_handle: Option<FocusHandle>,
    },
    Measured {
        size: Size<Pixels>,
        focus_handle: Option<FocusHandle>,
    },
}

impl ListItem {
    fn size(&self) -> Option<Size<Pixels>> {
        if let ListItem::Measured { size, .. } = self {
            Some(*size)
        } else {
            None
        }
    }

    fn size_hint(&self) -> Option<Size<Pixels>> {
        match self {
            ListItem::Measured { size, .. } => Some(*size),
            ListItem::Unmeasured { size_hint, .. } => *size_hint,
        }
    }

    fn with_default_size_hint(&self, default_size_hint: Size<Pixels>) -> Self {
        match self {
            ListItem::Unmeasured {
                size_hint,
                focus_handle,
            } => ListItem::Unmeasured {
                size_hint: size_hint.or(Some(default_size_hint)),
                focus_handle: focus_handle.clone(),
            },
            ListItem::Measured { size, focus_handle } => ListItem::Measured {
                size: *size,
                focus_handle: focus_handle.clone(),
            },
        }
    }

    fn as_unmeasured_with_hint(&self, default_size_hint: Option<Size<Pixels>>) -> Self {
        ListItem::Unmeasured {
            size_hint: self.size_hint().or(default_size_hint),
            focus_handle: self.focus_handle(),
        }
    }

    fn focus_handle(&self) -> Option<FocusHandle> {
        match self {
            ListItem::Unmeasured { focus_handle, .. } | ListItem::Measured { focus_handle, .. } => {
                focus_handle.clone()
            }
        }
    }

    fn contains_focused(&self, window: &Window, cx: &App) -> bool {
        match self {
            ListItem::Unmeasured { focus_handle, .. } | ListItem::Measured { focus_handle, .. } => {
                focus_handle
                    .as_ref()
                    .is_some_and(|handle| handle.contains_focused(window, cx))
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ListItemSummary {
    count: usize,
    rendered_count: usize,
    unrendered_count: usize,
    height: Pixels,
    has_focus_handles: bool,
    has_unknown_height: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Count(usize);

#[derive(Clone, Debug, Default)]
struct Height(Pixels);

impl std::fmt::Debug for ListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unmeasured { .. } => write!(f, "Unrendered"),
            Self::Measured { size, .. } => f.debug_struct("Rendered").field("size", size).finish(),
        }
    }
}

/// An offset into the MdList's items, in terms of the item index and the number
/// of pixels off the top left of the item.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListOffset {
    /// The index of an item in the MdList
    pub item_ix: usize,
    /// The number of pixels to offset from the item index.
    pub offset_in_item: Pixels,
}

impl Element for MdList {
    type RequestLayoutState = ();
    type PrepaintState = ListPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = match self.sizing_behavior {
            ListSizingBehavior::Infer => {
                let mut style = Style::default();
                style.overflow.y = Overflow::Scroll;
                style.refine(&self.style);
                window.with_text_style(style.text_style().cloned(), |window| {
                    let state = &mut *self.state.0.borrow_mut();

                    let available_height = if let Some(last_bounds) = state.last_layout_bounds {
                        last_bounds.size.height
                    } else {
                        // If we don't have the last layout bounds (first render),
                        // we might just use the overdraw value as the available height to layout enough items.
                        state.overdraw
                    };
                    let padding = style.padding.to_pixels(
                        state.last_layout_bounds.unwrap_or_default().size.into(),
                        window.rem_size(),
                    );

                    let layout_response = state.layout_items(
                        None,
                        available_height,
                        &padding,
                        &mut self.render_item,
                        window,
                        cx,
                    );
                    let max_element_width = layout_response.max_item_width;

                    let summary = state.items.summary();
                    let total_height = summary.height;

                    window.request_measured_layout(
                        style,
                        move |known_dimensions, available_space, _window, _cx| {
                            let width =
                                known_dimensions
                                    .width
                                    .unwrap_or(match available_space.width {
                                        AvailableSpace::Definite(x) => x,
                                        AvailableSpace::MinContent | AvailableSpace::MaxContent => {
                                            max_element_width
                                        }
                                    });
                            let height = match available_space.height {
                                AvailableSpace::Definite(height) => total_height.min(height),
                                AvailableSpace::MinContent | AvailableSpace::MaxContent => {
                                    total_height
                                }
                            };
                            size(width, height)
                        },
                    )
                })
            }
            ListSizingBehavior::Auto => {
                let mut style = Style::default();
                style.refine(&self.style);
                window.with_text_style(style.text_style().cloned(), |window| {
                    window.request_layout(style, None, cx)
                })
            }
        };
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> ListPrepaintState {
        let state = &mut *self.state.0.borrow_mut();
        state.reset = false;

        let mut style = Style::default();
        style.refine(&self.style);

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);

        // If the width of the MdList has changed, invalidate all cached item heights
        if state
            .last_layout_bounds
            .is_none_or(|last_bounds| last_bounds.size.width != bounds.size.width)
        {
            let default_size_hint = state.default_size_hint;
            let items: Vec<_> = state
                .items
                .iter()
                .map(|item| item.as_unmeasured_with_hint(default_size_hint))
                .collect();
            let new_items = SumTree::from_iter(items, ());
            state.items = new_items;
            state.measuring_behavior.reset();
        }

        let padding = style
            .padding
            .to_pixels(bounds.size.into(), window.rem_size());
        let layout =
            match state.prepaint_items(bounds, padding, true, &mut self.render_item, window, cx) {
                Ok(layout) => layout,
                Err(autoscroll_request) => {
                    state.logical_scroll_top = Some(autoscroll_request);
                    state
                        .prepaint_items(bounds, padding, false, &mut self.render_item, window, cx)
                        .unwrap()
                }
            };

        state.last_layout_bounds = Some(bounds);
        state.last_padding = Some(padding);
        ListPrepaintState { hitbox, layout }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let current_view = window.current_view();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for item in &mut prepaint.layout.item_layouts {
                item.element.paint(window, cx);
            }
        });

        let list_state = self.state.clone();
        let height = bounds.size.height;
        let scroll_top = prepaint.layout.scroll_top;
        let hitbox_id = prepaint.hitbox.id;
        let mut accumulated_scroll_delta = ScrollDelta::default();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox_id.should_handle_scroll(window) {
                accumulated_scroll_delta = accumulated_scroll_delta.coalesce(event.delta);
                let pixel_delta = accumulated_scroll_delta.pixel_delta(px(20.));
                list_state.0.borrow_mut().scroll(
                    &scroll_top,
                    height,
                    pixel_delta,
                    current_view,
                    window,
                    cx,
                )
            }
        });
    }
}

impl IntoElement for MdList {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Styled for MdList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl md_sum_tree::Item for ListItem {
    type Summary = ListItemSummary;

    fn summary(&self, _: ()) -> Self::Summary {
        match self {
            ListItem::Unmeasured {
                size_hint,
                focus_handle,
            } => ListItemSummary {
                count: 1,
                rendered_count: 0,
                unrendered_count: 1,
                height: if let Some(size) = size_hint {
                    size.height
                } else {
                    px(0.)
                },
                has_focus_handles: focus_handle.is_some(),
                has_unknown_height: size_hint.is_none(),
            },
            ListItem::Measured {
                size, focus_handle, ..
            } => ListItemSummary {
                count: 1,
                rendered_count: 1,
                unrendered_count: 0,
                height: size.height,
                has_focus_handles: focus_handle.is_some(),
                has_unknown_height: false,
            },
        }
    }
}

impl md_sum_tree::ContextLessSummary for ListItemSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.count += summary.count;
        self.rendered_count += summary.rendered_count;
        self.unrendered_count += summary.unrendered_count;
        self.height += summary.height;
        self.has_focus_handles |= summary.has_focus_handles;
        self.has_unknown_height |= summary.has_unknown_height;
    }
}

impl<'a> md_sum_tree::Dimension<'a, ListItemSummary> for Count {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ListItemSummary, _: ()) {
        self.0 += summary.count;
    }
}

impl<'a> md_sum_tree::Dimension<'a, ListItemSummary> for Height {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ListItemSummary, _: ()) {
        self.0 += summary.height;
    }
}

impl md_sum_tree::SeekTarget<'_, ListItemSummary, ListItemSummary> for Count {
    fn cmp(&self, other: &ListItemSummary, _: ()) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.count).unwrap()
    }
}

impl md_sum_tree::SeekTarget<'_, ListItemSummary, ListItemSummary> for Height {
    fn cmp(&self, other: &ListItemSummary, _: ()) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.height).unwrap()
    }
}
