//! Keyboard backlight controls (HID++ `0x1982`) for a keyboard's detail panel.
//!
//! An on/off toggle plus the two user-toggleable options MX Keys-class
//! keyboards expose — power-save (dim at critical battery) and the "wow"
//! power-on effect — persisted per device via [`AppState::commit_backlight`]
//! and pushed through [`crate::ipc_client::Command::SetBacklight`].

use gpui::{
    AnyElement, BorrowAppContext as _, Context, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, div,
};
use gpui_component::{h_flex, v_flex};
use openlogi_core::config::BacklightSettings;
use openlogi_hid::DeviceRoute;

use crate::components::device_read::issue_device_read;
use crate::components::status::{retry_line, status_line};
use crate::state::{AppState, BacklightLoad};
use crate::theme::{self, Palette, SelectableStyle, Typography as _};

/// Keyboard backlight panel: on/off + power-save/wow toggles.
pub struct BacklightPanel {
    _state_obs: Subscription,
}

impl BacklightPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state_obs = cx.observe_global::<AppState>(|_, cx| cx.notify());
        Self {
            _state_obs: state_obs,
        }
    }

    fn ensure_backlight_load(cx: &mut Context<Self>) {
        let Some((key, route)) = backlight_load_target(cx) else {
            return;
        };
        cx.update_global::<AppState, _>(|state, _| state.mark_backlight_loading(&key));
        issue_device_read(
            cx,
            key,
            route,
            crate::ipc_client::Command::ReadBacklight,
            AppState::store_backlight_settings,
            AppState::clear_backlight_loading,
        );
    }
}

impl Render for BacklightPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Self::ensure_backlight_load(cx);
        let pal = theme::palette(cx);
        let status = cx
            .try_global::<AppState>()
            .map_or(BacklightLoad::Unknown, AppState::current_backlight_status);

        let controls = match &status {
            BacklightLoad::Ready(backlight) => ready_controls(*backlight, pal),
            BacklightLoad::Unknown | BacklightLoad::Loading => status_line("…", pal),
            BacklightLoad::Failed(_) => {
                retry_line("backlight-retry", tr!("Unavailable"), pal, |cx| {
                    cx.update_global::<AppState, _>(|state, _| state.retry_active_backlight());
                    cx.refresh_windows();
                })
            }
            BacklightLoad::Unsupported(_) => status_line(tr!("Unavailable"), pal),
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
                            .child(tr!("BACKLIGHT")),
                    )
                    .child(match &status {
                        BacklightLoad::Ready(backlight) => toggle(*backlight, pal),
                        _ => status_line("…", pal),
                    }),
            )
            .child(controls)
    }
}

fn ready_controls(backlight: BacklightSettings, pal: Palette) -> AnyElement {
    v_flex()
        .gap_3()
        .w_full()
        .child(option_row(
            "backlight-power-save",
            tr!("Power-save"),
            backlight.power_save,
            |next: &mut BacklightSettings| next.power_save = !next.power_save,
            pal,
        ))
        .child(option_row(
            "backlight-wow",
            tr!("Wow effect"),
            backlight.wow,
            |next: &mut BacklightSettings| next.wow = !next.wow,
            pal,
        ))
        .into_any_element()
}

fn backlight_load_target(cx: &mut Context<BacklightPanel>) -> Option<(String, DeviceRoute)> {
    cx.try_global::<AppState>().and_then(|state| {
        if !state.current_backlight_unqueried() {
            return None;
        }
        let record = state.current_record()?;
        Some((record.config_key.clone(), record.route.clone()?))
    })
}

/// On/off pill — mirrors the lighting panel's toggle.
fn toggle(current: BacklightSettings, pal: Palette) -> AnyElement {
    let on = current.enabled;
    div()
        .id("backlight-toggle")
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
                let mut next = state.backlight();
                next.enabled = !next.enabled;
                state.commit_backlight(next);
            });
            cx.refresh_windows();
        })
        .into_any_element()
}

/// One labelled option row with a select pill on the right. `flip` mutates the
/// relevant field of a freshly-cloned [`BacklightSettings`] before commit.
fn option_row(
    id: &'static str,
    label: SharedString,
    on: bool,
    flip: fn(&mut BacklightSettings),
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
                let mut next = state.backlight();
                flip(&mut next);
                state.commit_backlight(next);
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
