//! Modal implementation for the codon which-key chord HUD.
//!
//! Ported from `vendor/zed/crates/which_key/src/which_key_modal.rs` with
//! three rendering changes spelled out in `REQ:codon/which-key-overlay`:
//!
//! 1. The HUD spans the full width of the active pane (via
//!    `workspace::codon_bridge::active_pane_bounds`) instead of the
//!    bottom-right 480 px corner.
//! 2. Bindings flow column-first across as many columns as fit
//!    (`pane_width / min_column_width`), so wide chord families like
//!    `g …` or `space …` don't have to scroll.
//! 3. The pending-keys title is prefixed with the current
//!    `CodonModeTracker` pane-mode (`[NORMAL]`, `[INSERT]`,
//!    `[COMMAND]`).
//!
//! `phase-16/which-key-auto-flip` extends the render with a top-anchor
//! variant when the natural content height would occlude more than
//! `flip_threshold` of the pane.

use std::collections::HashMap;

use codon_pane_bridge::{CodonModeTracker, PaneMode};
use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, FontWeight, Keystroke,
    Pixels, ScrollHandle, Subscription, WeakEntity, Window,
};
use settings::Settings;
use ui::{
    Divider, DividerColor, DynamicSpacing, WithScrollbar, prelude::*, text_for_keystrokes,
};
use workspace::{ModalView, Workspace, codon_bridge::active_pane_bounds};

use crate::FILTERED_KEYSTROKES;
use crate::codon_which_key_settings::CodonWhichKeySettings;

/// One rendered binding row — the keystrokes the user still needs to
/// press, followed by the action name.
pub(crate) type BindingRow = (SharedString, SharedString);

pub struct CodonWhichKeyModal {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    bindings: Vec<BindingRow>,
    pending_keys: SharedString,
    settings: CodonWhichKeySettings,
    _pending_input_subscription: Subscription,
    _focus_out_subscription: Subscription,
}

impl CodonWhichKeyModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        settings: CodonWhichKeySettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = window.focused(cx).unwrap_or(cx.focus_handle());
        let handle = cx.weak_entity();
        let mut this = Self {
            workspace,
            focus_handle: focus_handle.clone(),
            scroll_handle: ScrollHandle::new(),
            bindings: Vec::new(),
            pending_keys: SharedString::new_static(""),
            settings,
            _pending_input_subscription: cx.observe_pending_input(
                window,
                |this: &mut Self, window, cx| {
                    this.update_pending_keys(window, cx);
                },
            ),
            _focus_out_subscription: window.on_focus_out(&focus_handle, cx, move |_, _, cx| {
                handle.update(cx, |_, cx| cx.emit(DismissEvent)).ok();
            }),
        };
        this.update_pending_keys(window, cx);
        this
    }

    pub fn dismiss(&self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn update_pending_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending_keys) = window.pending_input_keystrokes() else {
            cx.emit(DismissEvent);
            return;
        };
        let bindings = window.possible_bindings_for_input(pending_keys);

        let mut binding_data = bindings
            .iter()
            .map(|binding| {
                (
                    binding
                        .keystrokes()
                        .iter()
                        .map(|k| k.inner().to_owned())
                        .collect::<Vec<_>>(),
                    binding.action(),
                )
            })
            .filter(|(keystrokes, _action)| {
                !FILTERED_KEYSTROKES.iter().any(|filtered| {
                    keystrokes.len() >= filtered.len()
                        && keystrokes[..filtered.len()] == filtered[..]
                })
            })
            .map(|(keystrokes, action)| {
                let remaining_keystrokes = keystrokes[pending_keys.len()..].to_vec();
                let action_name: SharedString =
                    command_palette::humanize_action_name(action.name()).into();
                (remaining_keystrokes, action_name)
            })
            .collect();

        binding_data = group_bindings(binding_data);

        // Stable sort: non-group first, then by keystroke count, then text length, then text.
        binding_data.sort_by(|(keystrokes_a, action_a), (keystrokes_b, action_b)| {
            let is_group_a = action_a.starts_with('+');
            let is_group_b = action_b.starts_with('+');

            let group_cmp = is_group_a.cmp(&is_group_b);
            if group_cmp != std::cmp::Ordering::Equal {
                return group_cmp;
            }

            let keystroke_cmp = keystrokes_a.len().cmp(&keystrokes_b.len());
            if keystroke_cmp != std::cmp::Ordering::Equal {
                return keystroke_cmp;
            }

            let text_a = text_for_keystrokes(keystrokes_a, cx);
            let text_b = text_for_keystrokes(keystrokes_b, cx);
            let text_len_cmp = text_a.len().cmp(&text_b.len());
            if text_len_cmp != std::cmp::Ordering::Equal {
                return text_len_cmp;
            }
            text_a.cmp(&text_b)
        });
        binding_data.dedup();

        self.pending_keys = text_for_keystrokes(&pending_keys, cx).into();
        self.bindings = binding_data
            .into_iter()
            .map(|(keystrokes, action)| (text_for_keystrokes(&keystrokes, cx).into(), action))
            .collect();
    }

    fn pane_mode_label(cx: &App) -> SharedString {
        let tracker = cx.global::<CodonModeTracker>();
        let mode = if tracker.command_active {
            PaneMode::Command
        } else {
            tracker.mode
        };
        match mode {
            PaneMode::Normal => SharedString::new_static("[NORMAL]"),
            PaneMode::Insert => SharedString::new_static("[INSERT]"),
            PaneMode::Command => SharedString::new_static("[COMMAND]"),
        }
    }
}

impl Render for CodonWhichKeyModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let has_rows = !self.bindings.is_empty();
        let viewport_size = window.viewport_size();

        let pane_bounds = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read_with(cx, |workspace, _| active_pane_bounds(workspace)));

        let status_height = self
            .workspace
            .upgrade()
            .and_then(|workspace| {
                workspace.read_with(cx, |workspace, cx| {
                    if workspace.status_bar_visible(cx) {
                        Some(
                            DynamicSpacing::Base04.px(cx) * 2.0
                                + theme_settings::ThemeSettings::get_global(cx).ui_font_size(cx),
                        )
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(px(0.));

        let pane_width = pane_bounds.map(|b| b.size.width).unwrap_or(viewport_size.width);
        let pane_height = pane_bounds
            .map(|b| b.size.height)
            .unwrap_or(viewport_size.height);
        let pane_origin_x = pane_bounds.map(|b| b.origin.x).unwrap_or(px(0.));
        let pane_origin_y = pane_bounds.map(|b| b.origin.y).unwrap_or(px(0.));

        let columns = compute_columns(
            pane_width,
            px(self.settings.min_column_width),
            self.bindings.len(),
        );

        let row_height = px(20.);
        let rows_per_column = if columns == 0 {
            0
        } else {
            self.bindings.len().div_ceil(columns)
        };
        let title_height = px(28.);
        let content_height = (row_height * rows_per_column as f32) + title_height + px(8.);
        let max_content_height = pane_height - status_height - px(12.);
        let clamped_height = content_height.min(max_content_height);

        // Bottom anchor is the default per `c-bottom-default`. The
        // auto-flip rule (`c-auto-flip` / TASK:phase-16/which-key-auto-flip)
        // lives in a follow-up commit and slots a top-anchor branch in here.
        let _ = pane_height;

        let mode_label = Self::pane_mode_label(cx);
        let pending_keys = self.pending_keys.clone();
        let title_section = h_flex()
            .gap(px(8.))
            .child(
                Label::new(mode_label)
                    .size(LabelSize::Default)
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::Muted),
            )
            .child(
                Label::new(pending_keys)
                    .size(LabelSize::Default)
                    .weight(FontWeight::MEDIUM)
                    .color(Color::Accent),
            );

        let title_wrapper = v_flex()
            .child(title_section)
            .when(has_rows, |el| {
                el.child(
                    div()
                        .child(Divider::horizontal().color(DividerColor::BorderFaded))
                        .mb(px(2.)),
                )
            });

        let column_views = (0..columns.max(1)).map(|column_index| {
            let start = column_index * rows_per_column;
            let end = (start + rows_per_column).min(self.bindings.len());
            let slice: Vec<BindingRow> = if start < end {
                self.bindings[start..end].to_vec()
            } else {
                Vec::new()
            };
            render_column(slice)
        });

        let content = h_flex()
            .id("codon-which-key-content")
            .items_start()
            .gap(px(16.))
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .max_h(clamped_height)
            .children(column_views);

        let panel_body = v_flex()
            .child(title_wrapper)
            .when(has_rows, |el| {
                el.child(
                    div()
                        .max_h(clamped_height)
                        .child(content)
                        .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                )
            });

        let panel = div()
            .id("codon-which-key-panel")
            .occlude()
            .absolute()
            .left(pane_origin_x)
            .w(pane_width)
            .elevation_3(cx)
            .px(px(8.))
            .py(px(4.))
            .child(panel_body);

        let margin_bottom = px(4.);
        let bottom_offset = margin_bottom + status_height;
        let pane_bottom = viewport_size.height - (pane_origin_y + pane_height);
        panel.bottom(pane_bottom + bottom_offset)
    }
}

impl EventEmitter<DismissEvent> for CodonWhichKeyModal {}

impl Focusable for CodonWhichKeyModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for CodonWhichKeyModal {
    fn render_bare(&self) -> bool {
        true
    }
}

fn render_column(rows: Vec<BindingRow>) -> gpui::AnyElement {
    use gpui::IntoElement;
    v_flex()
        .gap(px(2.))
        .flex_1()
        .min_w_0()
        .children(rows.into_iter().map(|(keys, action)| {
            let is_group = action.starts_with('+');
            let label_color = if is_group {
                Color::Success
            } else {
                Color::Default
            };
            h_flex()
                .gap(px(8.))
                .items_baseline()
                .child(
                    Label::new(keys)
                        .size(LabelSize::Default)
                        .color(Color::Accent),
                )
                .child(
                    Label::new(action)
                        .size(LabelSize::Default)
                        .color(label_color)
                        .single_line()
                        .truncate(),
                )
        }))
        .into_any_element()
}

/// Returns the number of columns the HUD should render with.
///
/// Single column when `pane_width` is below `min_column_width` or the
/// binding list is empty. Otherwise `floor(pane_width / min_column_width)`,
/// capped at `binding_count` (no empty columns).
pub fn compute_columns(pane_width: Pixels, min_column_width: Pixels, binding_count: usize) -> usize {
    if binding_count == 0 {
        return 1;
    }
    let min = f32::from(min_column_width).max(1.0);
    let available = f32::from(pane_width).max(0.0);
    let raw = (available / min).floor() as usize;
    raw.clamp(1, binding_count)
}

fn group_bindings(
    binding_data: Vec<(Vec<Keystroke>, SharedString)>,
) -> Vec<(Vec<Keystroke>, SharedString)> {
    let mut groups: HashMap<Option<Keystroke>, Vec<(Vec<Keystroke>, SharedString)>> =
        HashMap::new();

    for (remaining_keystrokes, action_name) in binding_data {
        let first_key = remaining_keystrokes.first().cloned();
        groups
            .entry(first_key)
            .or_default()
            .push((remaining_keystrokes, action_name));
    }

    let mut result = Vec::new();
    for (first_key, mut group_bindings) in groups {
        group_bindings.dedup_by_key(|(keystrokes, _)| keystrokes.clone());

        if let Some(first_key) = first_key
            && group_bindings.len() > 1
        {
            let first_keystroke = vec![first_key];
            let count = group_bindings.len();
            result.push((first_keystroke, format!("+{} keybinds", count).into()));
        } else {
            result.append(&mut group_bindings);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn compute_columns_zero_bindings_is_single_column() {
        assert_eq!(compute_columns(px(1000.), px(240.), 0), 1);
    }

    #[test]
    fn compute_columns_narrow_pane_is_single() {
        assert_eq!(compute_columns(px(100.), px(240.), 12), 1);
    }

    #[test]
    fn compute_columns_wide_pane_floors_to_capacity() {
        // 1000 / 240 = 4.16 → floor 4
        assert_eq!(compute_columns(px(1000.), px(240.), 12), 4);
    }

    #[test]
    fn compute_columns_caps_at_binding_count() {
        assert_eq!(compute_columns(px(2000.), px(240.), 3), 3);
    }

}
