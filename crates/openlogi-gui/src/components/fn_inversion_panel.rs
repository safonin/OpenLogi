//! Fn-lock (function-key inversion, HID++ `0x40a3`) controls for a keyboard's
//! detail panel. A single on/off toggle, persisted per device via
//! [`AppState::commit_fn_lock`] and pushed through
//! [`crate::ipc_client::Command::SetFnInversion`].

use gpui::{
    AnyElement, BorrowAppContext as _, Context, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement as _, Styled, Subscription, Window, div,
};
use gpui_component::{h_flex, v_flex};
use openlogi_core::config::FnLock;
use openlogi_hid::DeviceRoute;

use crate::components::device_read::issue_device_read;
use crate::components::status::{retry_line, status_line};
use crate::state::{AppState, FnLockLoad};
use crate::theme::{self, Palette, SelectableStyle, Typography as _};

/// Fn-lock panel: a single on/off toggle.
pub struct FnInversionPanel {
    _state_obs: Subscription,
}

impl FnInversionPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state_obs = cx.observe_global::<AppState>(|_, cx| cx.notify());
        Self {
            _state_obs: state_obs,
        }
    }

    fn ensure_fn_lock_load(cx: &mut Context<Self>) {
        let Some((key, route)) = fn_lock_load_target(cx) else {
            return;
        };
        cx.update_global::<AppState, _>(|state, _| state.mark_fn_lock_loading(&key));
        issue_device_read(
            cx,
            key,
            route,
            crate::ipc_client::Command::ReadFnInversion,
            AppState::store_fn_lock,
            AppState::clear_fn_lock_loading,
        );
    }
}

impl Render for FnInversionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Self::ensure_fn_lock_load(cx);
        let pal = theme::palette(cx);
        let status = cx
            .try_global::<AppState>()
            .map_or(FnLockLoad::Unknown, AppState::current_fn_lock_status);

        let control = match status {
            FnLockLoad::Ready(lock) => toggle(lock, pal),
            FnLockLoad::Unknown | FnLockLoad::Loading => status_line("…", pal),
            FnLockLoad::Failed(_) => retry_line("fn-lock-retry", tr!("Unavailable"), pal, |cx| {
                cx.update_global::<AppState, _>(|state, _| state.retry_active_fn_lock());
                cx.refresh_windows();
            }),
            FnLockLoad::Unsupported(_) => status_line(tr!("Unavailable"), pal),
        };

        v_flex()
            .gap_3()
            .w_full()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_body()
                            .text_color(pal.text_muted)
                            .child(tr!("FN LOCK")),
                    )
                    .child(control),
            )
            .child(
                div()
                    .text_caption()
                    .text_color(pal.text_muted)
                    .child(tr!("Lock the F-keys to their primary (F1–F12) function.")),
            )
    }
}

fn fn_lock_load_target(cx: &mut Context<FnInversionPanel>) -> Option<(String, DeviceRoute)> {
    cx.try_global::<AppState>().and_then(|state| {
        if !state.current_fn_lock_unqueried() {
            return None;
        }
        let record = state.current_record()?;
        Some((record.config_key.clone(), record.route.clone()?))
    })
}

/// On/off pill — mirrors the backlight panel's toggle.
fn toggle(current: FnLock, pal: Palette) -> AnyElement {
    let on = current.enabled;
    div()
        .id("fn-lock-toggle")
        .px_2()
        .py_1()
        .rounded(pal.control_radius)
        .selected_border(on, pal)
        .selected_fill(on)
        .text_caption()
        .text_color(if on { pal.text_primary } else { pal.text_muted })
        .cursor_pointer()
        .child(if on { tr!("On") } else { tr!("Off") })
        .on_click(|_event, _window, cx| {
            cx.update_global::<AppState, _>(|state, _| {
                let mut next = state.fn_lock();
                next.enabled = !next.enabled;
                state.commit_fn_lock(next);
            });
            cx.refresh_windows();
        })
        .into_any_element()
}
