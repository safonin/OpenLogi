//! Key remap panel (HID++ `0x1b04`) for a keyboard's detail tab.
//!
//! Lists the device's divertable controls and their current remap targets,
//! fetched from the agent via `read_remappable_controls`. A reset button
//! clears the saved remap for a key. Full remap editing (target picker)
//! is a follow-up.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, BorrowAppContext as _, Context, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, div,
};
use gpui_component::{h_flex, v_flex};
use openlogi_core::config::KeyRemap;
use openlogi_hid::{DeviceRoute, RemappableControl};

use crate::components::device_read::issue_device_read;
use crate::components::status::{retry_line, status_line};
use crate::state::{AppState, RemapControlsLoad};
use crate::theme::{self, Palette, Typography as _};

/// Key remap panel: lists divertable controls and their remap state.
pub struct RemapPanel {
    _state_obs: Subscription,
}

impl RemapPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state_obs = cx.observe_global::<AppState>(|_, cx| cx.notify());
        Self {
            _state_obs: state_obs,
        }
    }

    fn ensure_controls_load(cx: &mut Context<Self>) {
        let Some((key, route)) = remap_controls_load_target(cx) else {
            return;
        };
        cx.update_global::<AppState, _>(|state, _| state.mark_remap_controls_loading(&key));
        issue_device_read(
            cx,
            key,
            route,
            crate::ipc_client::Command::ReadRemappableControls,
            AppState::store_remap_controls,
            AppState::clear_remap_controls_loading,
        );
    }
}

impl Render for RemapPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Self::ensure_controls_load(cx);
        let pal = theme::palette(cx);
        let (remap, status) = cx.try_global::<AppState>().map_or_else(
            || (KeyRemap::default(), RemapControlsLoad::Unknown),
            |state| (state.key_remap(), state.current_remap_controls_status()),
        );
        let controls = match status {
            RemapControlsLoad::Ready(controls) => controls_list(&controls, &remap, pal),
            RemapControlsLoad::Unknown | RemapControlsLoad::Loading => status_line("…", pal),
            RemapControlsLoad::Failed(_) => {
                retry_line("remap-controls-retry", tr!("Unavailable"), pal, |cx| {
                    cx.update_global::<AppState, _>(|state, _| {
                        state.retry_active_remap_controls();
                    });
                    cx.refresh_windows();
                })
            }
            RemapControlsLoad::Unsupported(_) => status_line(tr!("Unavailable"), pal),
        };

        v_flex()
            .gap_3()
            .w_full()
            .child(
                div()
                    .text_body()
                    .text_color(pal.text_muted)
                    .child(tr!("KEY REMAP")),
            )
            .child(controls)
    }
}

fn controls_list(controls: &[RemappableControl], remap: &KeyRemap, pal: Palette) -> AnyElement {
    let controls = controls
        .iter()
        .filter(|control| control.divertable)
        .cloned()
        .collect::<Vec<_>>();
    if controls.is_empty() {
        return div()
            .text_caption()
            .text_color(pal.text_muted)
            .child(tr!("Unavailable"))
            .into_any_element();
    }
    let rows = controls
        .iter()
        .map(|source| remap_row(source, &controls, remap.0.get(&source.cid).copied(), pal))
        .collect::<Vec<_>>();
    v_flex().gap_1().children(rows).into_any_element()
}

fn remap_row(
    source: &RemappableControl,
    controls: &[RemappableControl],
    target: Option<u16>,
    pal: Palette,
) -> AnyElement {
    let source_id = source.cid;
    let targets = controls
        .iter()
        .map(|control| control.cid)
        .collect::<Vec<_>>();
    let target_label = target
        .and_then(|target| controls.iter().find(|control| control.cid == target))
        .map_or_else(|| SharedString::from("—"), cid_label);
    h_flex()
        .justify_between()
        .items_center()
        .py_1()
        .child(
            div()
                .text_caption()
                .text_color(pal.text_primary)
                .child(cid_label(source)),
        )
        .child(
            h_flex()
                .gap_1()
                .child(
                    div()
                        .id(("remap-target", source_id as usize))
                        .px_2()
                        .py_1()
                        .rounded(pal.control_radius)
                        .border_1()
                        .border_color(pal.border)
                        .text_caption()
                        .cursor_pointer()
                        .child(target_label)
                        .on_click(move |_event, _window, cx| {
                            let next_target = next_target(source_id, target, &targets);
                            cx.update_global::<AppState, _>(|state, _| {
                                let mut next = state.key_remap();
                                if let Some(next_target) = next_target {
                                    next.0.insert(source_id, next_target);
                                }
                                state.commit_key_remap(next);
                            });
                            cx.refresh_windows();
                        }),
                )
                .when(target.is_some(), |row| {
                    row.child(
                        div()
                            .id(("remap-reset", source_id as usize))
                            .px_2()
                            .py_1()
                            .text_caption()
                            .text_color(pal.text_muted)
                            .cursor_pointer()
                            .child(tr!("Reset"))
                            .on_click(move |_event, _window, cx| {
                                cx.update_global::<AppState, _>(|state, _| {
                                    let mut next = state.key_remap();
                                    next.0.remove(&source_id);
                                    state.commit_key_remap(next);
                                });
                                cx.refresh_windows();
                            }),
                    )
                }),
        )
        .into_any_element()
}

fn next_target(source: u16, current: Option<u16>, targets: &[u16]) -> Option<u16> {
    let candidates = targets
        .iter()
        .copied()
        .filter(|target| *target != source)
        .collect::<Vec<_>>();
    let next = current
        .and_then(|current| candidates.iter().position(|target| *target == current))
        .map_or(0, |position| (position + 1) % candidates.len().max(1));
    candidates.get(next).copied()
}

fn remap_controls_load_target(cx: &mut Context<RemapPanel>) -> Option<(String, DeviceRoute)> {
    cx.try_global::<AppState>().and_then(|state| {
        if !state.current_remap_controls_unqueried() {
            return None;
        }
        let record = state.current_record()?;
        Some((record.config_key.clone(), record.route.clone()?))
    })
}

/// A displayable name for a [`RemappableControl`]'s CID, when known.
/// Falls back to the hex CID for unmapped controls.
///
#[must_use]
pub fn cid_label(ctrl: &RemappableControl) -> SharedString {
    // A small set of the common MX Keys Fn-row controls; the rest show as hex.
    let name = match ctrl.cid {
        0x00d1 => "Host 1",
        0x00d2 => "Host 2",
        0x00d3 => "Host 3",
        0x00c7 => "Brightness Down",
        0x00c8 => "Brightness Up",
        0x00e0 => "Mission Control",
        0x00e1 => "Launchpad",
        0x00e2 => "Backlight −",
        0x00e3 => "Backlight +",
        0x00e4 => "Previous Track",
        0x00e5 => "Play/Pause",
        0x00e6 => "Next Track",
        0x00e7 => "Mute",
        0x00e8 => "Volume Down",
        0x00e9 => "Volume Up",
        0x00bf => "Screen Capture",
        0x00ea => "Context Menu",
        0x00eb => "Left Arrow",
        0x00ec => "Right Arrow",
        0x00de => "F-Lock",
        _ => return SharedString::from(format!("0x{:04x}", ctrl.cid)),
    };
    SharedString::from(name)
}

#[cfg(test)]
mod tests {
    use super::next_target;

    #[test]
    fn target_cycle_skips_the_source_and_wraps() {
        let targets = [1, 2, 3];
        assert_eq!(next_target(2, None, &targets), Some(1));
        assert_eq!(next_target(2, Some(1), &targets), Some(3));
        assert_eq!(next_target(2, Some(3), &targets), Some(1));
        assert_eq!(next_target(1, None, &[1]), None);
    }
}
