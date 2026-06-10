//! Opt-in status-bar token counter
//! (REQ:codon/agent-harness#c-cost-bookkeeping).
//!
//! Renders the session's running `↓ <in> ↑ <out>` totals from the
//! [`TraceLog`] accumulator. Gated behind `[agent_harness]
//! show_token_counter = true` in codon.toml — with the default config
//! the item renders nothing at all, keeping codon terminal-quiet. The
//! config watcher re-applies [`HarnessSettings`] on every edit, so
//! flipping the flag takes effect without a restart.

use crate::runtime::{HarnessSettings, TraceLog};
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription,
    Window, div,
};
use ui::{Color, Label, LabelCommon, LabelSize, h_flex};
use workspace::{ItemHandle, StatusItemView, Workspace};

pub struct TokenStatusItem {
    _observe_trace: Subscription,
}

impl TokenStatusItem {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // TraceLog::push goes through update_global, which fires this
        // observer — the counter refreshes once per completed turn.
        let observe = cx.observe_global::<TraceLog>(|_, cx| cx.notify());
        Self {
            _observe_trace: observe,
        }
    }

    /// Mount on the workspace status bar. The item is always mounted;
    /// visibility is decided at render time by [`HarnessSettings`] so
    /// config edits toggle it live.
    pub fn register(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<TokenStatusItem> {
        let item = cx.new(TokenStatusItem::new);
        let handle = item.clone();
        workspace.status_bar().update(cx, |status_bar, cx| {
            status_bar.add_right_item(item, window, cx);
        });
        handle
    }
}

/// Compact human form: 1234 → "1.2k", 1234567 → "1.2M".
fn compact(count: u64) -> String {
    match count {
        0..=9_999 => count.to_string(),
        10_000..=999_999 => format!("{:.1}k", count as f64 / 1_000.0),
        _ => format!("{:.1}M", count as f64 / 1_000_000.0),
    }
}

impl Render for TokenStatusItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !HarnessSettings::show_token_counter(cx) {
            return div().into_any_element();
        }
        let (tokens_in, tokens_out) = TraceLog::token_totals(cx);
        h_flex()
            .gap_1()
            .child(
                Label::new(format!(
                    "↓ {} ↑ {}",
                    compact(tokens_in),
                    compact(tokens_out)
                ))
                .color(Color::Muted)
                .size(LabelSize::Small),
            )
            .into_any_element()
    }
}

impl StatusItemView for TokenStatusItem {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_formats_magnitudes() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(9_999), "9999");
        assert_eq!(compact(12_345), "12.3k");
        assert_eq!(compact(1_234_567), "1.2M");
    }
}
