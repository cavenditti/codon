//! Generic adapter that hosts any Zed `workspace::Panel` as a `workspace::Item`.
//!
//! The `Panel` trait has dock-host concerns baked in (position, default_size,
//! toggle_action, …) that are no-ops when the host is a pane. The adapter only
//! consumes the load-bearing subset: `Focusable + Render`, `persistent_name`,
//! `icon` / `icon_label`, and `set_active` for tab activation.

use std::marker::PhantomData;

use gpui::{
    AnyElement, App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window, div,
};
use ui::{Icon, IconName, Label, prelude::*};
use workspace::{
    Pane, Workspace,
    dock::{Panel, PanelEvent},
    item::{Item, ItemEvent, TabContentParams},
};

/// Wraps an existing `Entity<P>` so it can be inserted into a pane as a
/// regular workspace item. The wrapper is transparent: `Render` and
/// `Focusable` forward to the inner panel, and `PanelEvent::Activate /
/// Close` are translated into the matching `ItemEvent` flavours so the
/// pane reacts correctly.
pub struct PanelItemAdapter<P: Panel> {
    inner: Entity<P>,
    focus_handle: FocusHandle,
    _panel_subscription: Subscription,
    _phantom: PhantomData<P>,
}

impl<P: Panel> PanelItemAdapter<P> {
    pub fn new(inner: Entity<P>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = inner.read(cx).focus_handle(cx);
        let panel_subscription = cx.subscribe(&inner, Self::on_panel_event);
        Self {
            inner,
            focus_handle,
            _panel_subscription: panel_subscription,
            _phantom: PhantomData,
        }
    }

    /// Borrow the inner panel entity. Useful for routing focus after seed
    /// helpers (e.g. `seed_explain_with_selection` on the agent panel).
    pub fn inner(&self) -> &Entity<P> {
        &self.inner
    }

    /// Stable id of the inner panel entity. Lets callers walk a workspace's
    /// pane tree and dedupe by panel identity.
    pub fn inner_id(&self) -> EntityId {
        self.inner.entity_id()
    }

    fn on_panel_event(
        &mut self,
        _entity: Entity<P>,
        event: &PanelEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PanelEvent::Activate => cx.emit(PanelItemEvent::Activate),
            PanelEvent::Close => cx.emit(PanelItemEvent::Close),
            // Zoom is a workspace concern; ignore the panel's zoom intent
            // when it's hosted as an Item. The user zooms the pane instead.
            PanelEvent::ZoomIn | PanelEvent::ZoomOut => {}
        }
    }
}

/// Events the adapter emits. Item-style flavours only — `PanelEvent::ZoomIn`
/// / `ZoomOut` are dropped because zoom is the workspace's job once the
/// panel is hosted by a pane.
#[derive(Debug, Clone, Copy)]
pub enum PanelItemEvent {
    Activate,
    Close,
}

impl<P: Panel> EventEmitter<PanelItemEvent> for PanelItemAdapter<P> {}

impl<P: Panel> Focusable for PanelItemAdapter<P> {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<P: Panel> Render for PanelItemAdapter<P> {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Mount the inner panel entity as the adapter's only child. The
        // adapter itself is a thin div so Item-level styling / focus rings
        // can be applied without rewriting the panel.
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .child(self.inner.clone())
    }
}

impl<P: Panel> Item for PanelItemAdapter<P> {
    type Event = PanelItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        SharedString::from(P::persistent_name())
    }

    fn tab_content(
        &self,
        params: TabContentParams,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let inner = self.inner.read(cx);
        let label = inner
            .icon_label(window, cx)
            .map(SharedString::from)
            .unwrap_or_else(|| SharedString::from(P::persistent_name()));
        Label::new(label)
            .color(params.text_color())
            .into_any_element()
    }

    fn tab_icon(&self, window: &Window, cx: &App) -> Option<Icon> {
        self.inner
            .read(cx)
            .icon(window, cx)
            .map(|name: IconName| Icon::new(name))
    }

    fn tab_tooltip_text(&self, _: &App) -> Option<SharedString> {
        Some(SharedString::from(P::persistent_name()))
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some(P::persistent_name())
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inner.update(cx, |panel, cx| {
            panel.set_active(false, window, cx);
        });
    }

    fn added_to_workspace(
        &mut self,
        _workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inner.update(cx, |panel, cx| {
            panel.set_active(true, window, cx);
        });
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            PanelItemEvent::Close => f(ItemEvent::CloseItem),
            PanelItemEvent::Activate => f(ItemEvent::Edit),
        }
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<workspace::WorkspaceId>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> gpui::Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        // Panels are workspace-scoped singletons; splitting the tab is a
        // no-op (matches Zed's dock-host semantics).
        gpui::Task::ready(None)
    }

    fn is_dirty(&self, _cx: &App) -> bool {
        false
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: std::any::TypeId,
        self_handle: &'a Entity<Self>,
        _cx: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if std::any::TypeId::of::<Self>() == type_id {
            Some(self_handle.clone().into())
        } else if std::any::TypeId::of::<P>() == type_id {
            Some(self.inner.clone().into())
        } else {
            None
        }
    }
}

/// Re-export of the inner panel's pane (when one exists). Used by callers
/// that want to forward focus into a sub-pane owned by the panel itself
/// (e.g. `OutlinePanel` exposes its own internal pane).
pub fn inner_pane<P: Panel>(adapter: &PanelItemAdapter<P>, cx: &App) -> Option<Entity<Pane>> {
    adapter.inner.read(cx).pane()
}

