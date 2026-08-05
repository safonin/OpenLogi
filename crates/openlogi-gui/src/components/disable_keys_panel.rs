//! Disabled-keys panel (HID++ `0x4521`) for a keyboard's detail tab.
//!
//! A checkbox per disableable key (Caps Lock, Num Lock, Scroll Lock, Insert,
//! Win), persisted per device via [`AppState::commit_disabled_keys`] and pushed
//! through [`crate::ipc_client::Command::SetDisabledKeys`].

use gpui::{
    AnyElement, BorrowAppContext as _, Context, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, div,
};
use gpui_component::{h_flex, v_flex};
use openlogi_core::config::DisabledKeys;
use openlogi_hid::DeviceRoute;

use crate::components::device_read::issue_device_read;
use crate::components::status::{retry_line, status_line};
use crate::state::{AppState, DisabledKeysLoad};
use crate::theme::{self, Palette, SelectableStyle, Typography as _};

/// Disabled-keys panel: one checkbox per disableable key.
pub struct DisableKeysPanel {
    _state_obs: Subscription,
}

impl DisableKeysPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state_obs = cx.observe_global::<AppState>(|_, cx| cx.notify());
        Self {
            _state_obs: state_obs,
        }
    }

    fn ensure_disabled_keys_load(cx: &mut Context<Self>) {
        let Some((key, route)) = disabled_keys_load_target(cx) else {
            return;
        };
        cx.update_global::<AppState, _>(|state, _| state.mark_disabled_keys_loading(&key));
        issue_device_read(
            cx,
            key,
            route,
            crate::ipc_client::Command::ReadDisabledKeys,
            AppState::store_disabled_keys,
            AppState::clear_disabled_keys_loading,
        );
    }
}

impl Render for DisableKeysPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Self::ensure_disabled_keys_load(cx);
        let pal = theme::palette(cx);
        let status = cx.try_global::<AppState>().map_or(
            DisabledKeysLoad::Unknown,
            AppState::current_disabled_keys_status,
        );

        let controls = match status {
            DisabledKeysLoad::Ready(keys) => ready_controls(keys, pal),
            DisabledKeysLoad::Unknown | DisabledKeysLoad::Loading => status_line("…", pal),
            DisabledKeysLoad::Failed(_) => {
                retry_line("disable-keys-retry", tr!("Unavailable"), pal, |cx| {
                    cx.update_global::<AppState, _>(|state, _| state.retry_active_disabled_keys());
                    cx.refresh_windows();
                })
            }
            DisabledKeysLoad::Unsupported(_) => status_line(tr!("Unavailable"), pal),
        };

        v_flex()
            .gap_3()
            .w_full()
            .child(
                div()
                    .text_body()
                    .text_color(pal.text_muted)
                    .child(tr!("DISABLE KEYS")),
            )
            .child(controls)
    }
}

fn ready_controls(keys: DisabledKeys, pal: Palette) -> AnyElement {
    v_flex()
        .gap_3()
        .w_full()
        .child(option_row(
            "disable-caps-lock",
            tr!("Caps Lock"),
            keys.caps_lock,
            |k| k.caps_lock = !k.caps_lock,
            pal,
        ))
        .child(option_row(
            "disable-num-lock",
            tr!("Num Lock"),
            keys.num_lock,
            |k| k.num_lock = !k.num_lock,
            pal,
        ))
        .child(option_row(
            "disable-scroll-lock",
            tr!("Scroll Lock"),
            keys.scroll_lock,
            |k| k.scroll_lock = !k.scroll_lock,
            pal,
        ))
        .child(option_row(
            "disable-insert",
            tr!("Insert"),
            keys.insert,
            |k| k.insert = !k.insert,
            pal,
        ))
        .child(option_row(
            "disable-windows",
            tr!("Windows / Cmd"),
            keys.windows,
            |k| k.windows = !k.windows,
            pal,
        ))
        .into_any_element()
}

fn disabled_keys_load_target(cx: &mut Context<DisableKeysPanel>) -> Option<(String, DeviceRoute)> {
    cx.try_global::<AppState>().and_then(|state| {
        if !state.current_disabled_keys_unqueried() {
            return None;
        }
        let record = state.current_record()?;
        Some((record.config_key.clone(), record.route.clone()?))
    })
}

/// One labelled row with an on/off pill. `flip` toggles the relevant field of a
/// freshly-cloned [`DisabledKeys`] before commit.
fn option_row(
    id: &'static str,
    label: SharedString,
    on: bool,
    flip: fn(&mut DisabledKeys),
    pal: Palette,
) -> AnyElement {
    h_flex()
        .id(id)
        .justify_between()
        .items_center()
        .py_1()
        .cursor_pointer()
        .on_click(move |_event, _window, cx| {
            cx.update_global::<AppState, _>(|state, _| {
                let mut next = state.disabled_keys();
                flip(&mut next);
                state.commit_disabled_keys(next);
            });
            cx.refresh_windows();
        })
        .child(
            div()
                .text_caption()
                .text_color(pal.text_primary)
                .child(label),
        )
        .child(
            div()
                .px_2()
                .py_1()
                .rounded(pal.control_radius)
                .selected_border(on, pal)
                .selected_fill(on)
                .text_caption()
                .text_color(if on { pal.text_primary } else { pal.text_muted })
                .child(if on { tr!("On") } else { tr!("Off") }),
        )
        .into_any_element()
}
