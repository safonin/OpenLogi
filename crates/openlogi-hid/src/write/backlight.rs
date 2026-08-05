//! HID++ `Backlight2` (`0x1982`) — adjustable keyboard backlight (MX Keys,
//! Craft, …). Reads the current config/info and writes mode, effect, and the
//! on/off toggle.
//!
//! The HID++ feature itself is implemented in `openlogi-hidpp`; this module is
//! the route-resolved entry point the agent and CLI use, mirroring
//! [`super::dpi`] / [`super::lighting`].

use std::sync::Arc;

use hidpp::{
    device::Device,
    feature::{
        CreatableFeature,
        backlight::{
            BacklightConfig, BacklightFeature, BacklightInfo, BacklightMode, BacklightOptions,
            SetBacklightConfig,
        },
    },
};
use openlogi_core::config::BacklightSettings;
use tracing::debug;

use crate::route::DeviceRoute;

use super::{HidppOperation, WriteError, classify_hidpp_error, open_feature, with_route};

/// Read the device's current backlight configuration — the on/off state, mode,
/// supported effects, current manual level, and the three fade-out durations.
pub async fn get_backlight_config(route: &DeviceRoute) -> Result<BacklightConfig, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<BacklightFeature>(&mut device).await?;
        feature.get_backlight_config().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadBacklight, BacklightFeature::ID)
        })
    })
    .await
}

/// Read general backlight information: the number of selectable levels, the
/// current level/status/effect, and the out-of-box fade-out durations.
pub async fn get_backlight_info(route: &DeviceRoute) -> Result<BacklightInfo, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<BacklightFeature>(&mut device).await?;
        feature.get_backlight_info().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadBacklight, BacklightFeature::ID)
        })
    })
    .await
}

/// Persistently write a full backlight configuration to the device's
/// non-volatile memory. Fields the firmware does not support (manual level and
/// fade-out durations on version-1 devices) are silently ignored by the
/// device — see the HID++ feature's version note.
pub async fn set_backlight_config(
    route: &DeviceRoute,
    config: hidpp::feature::backlight::SetBacklightConfig,
) -> Result<(), WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<BacklightFeature>(&mut device).await?;
        feature.set_backlight_config(config).await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::WriteBacklight, BacklightFeature::ID)
        })?;
        debug!(index, ?config, "wrote backlight config");
        Ok(())
    })
    .await
}

/// Write the user-facing [`BacklightSettings`] (on/off + power-save/wow
/// toggles) to the device, mapping onto the HID++ config write.
///
/// Read-merge-write: the device's existing `current_level`, fade-out
/// durations, mode, and effect are read back and preserved, so this only
/// flips the user-facing `enabled`/`power_save`/`wow` fields. This is safe on
/// every version: on v1 firmware the preserved level/duration fields are
/// ignored on write (per the feature version note), and on v3 (e.g. the Craft)
/// it avoids wiping the user's manual brightness and fade-out timers.
pub async fn set_backlight_settings(
    route: &DeviceRoute,
    settings: &BacklightSettings,
) -> Result<(), WriteError> {
    // Read the current config so we preserve the fields the GUI doesn't own.
    // If the read fails there is no safe value for fields this UI does not
    // own, so fail closed instead of replacing them with zeroes.
    let preserved = PreservedBacklightConfig::from(get_backlight_config(route).await?);
    let config = merge_backlight_settings(preserved, *settings);
    set_backlight_config(route, config).await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreservedBacklightConfig {
    options: BacklightOptions,
    mode: BacklightMode,
    current_level: u8,
    duration_hands_out: u16,
    duration_hands_in: u16,
    duration_powered: u16,
}

impl From<BacklightConfig> for PreservedBacklightConfig {
    fn from(config: BacklightConfig) -> Self {
        Self {
            options: config.options,
            mode: config.mode,
            current_level: config.current_level,
            duration_hands_out: config.duration_hands_out,
            duration_hands_in: config.duration_hands_in,
            duration_powered: config.duration_powered,
        }
    }
}

fn merge_backlight_settings(
    preserved: PreservedBacklightConfig,
    settings: BacklightSettings,
) -> SetBacklightConfig {
    // Crown is not exposed by this UI, so keep its current writable value.
    // Capability bits are read-only and must not be echoed into a write.
    let mut options = preserved.options & BacklightOptions::CROWN;
    if settings.power_save {
        options |= BacklightOptions::PWR_SAVE;
    }
    if settings.wow {
        options |= BacklightOptions::WOW;
    }
    SetBacklightConfig {
        enabled: settings.enabled,
        options,
        mode: preserved.mode,
        effect: None,
        current_level: preserved.current_level,
        duration_hands_out: preserved.duration_hands_out,
        duration_hands_in: preserved.duration_hands_in,
        duration_powered: preserved.duration_powered,
    }
}

/// Read the current backlight config from `route` and collapse the HID++
/// options/mode into the user-facing [`BacklightSettings`].
pub async fn read_backlight_settings(route: &DeviceRoute) -> Result<BacklightSettings, WriteError> {
    let cfg = get_backlight_config(route).await?;
    Ok(BacklightSettings {
        enabled: cfg.enabled,
        power_save: cfg.options.contains(BacklightOptions::PWR_SAVE),
        wow: cfg.options.contains(BacklightOptions::WOW),
    })
}

#[cfg(test)]
mod tests {
    use super::{PreservedBacklightConfig, merge_backlight_settings};
    use hidpp::feature::backlight::{BacklightMode, BacklightOptions};
    use openlogi_core::config::BacklightSettings;

    #[test]
    fn merge_preserves_unowned_backlight_fields() {
        let preserved = PreservedBacklightConfig {
            options: BacklightOptions::CROWN
                | BacklightOptions::WOW
                | BacklightOptions::WOW_SUPPORTED,
            mode: BacklightMode::PermanentManual,
            current_level: 6,
            duration_hands_out: 11,
            duration_hands_in: 22,
            duration_powered: 33,
        };

        let merged = merge_backlight_settings(
            preserved,
            BacklightSettings {
                enabled: false,
                power_save: true,
                wow: false,
            },
        );

        assert!(!merged.enabled);
        assert_eq!(
            merged.options,
            BacklightOptions::CROWN | BacklightOptions::PWR_SAVE
        );
        assert_eq!(merged.mode, BacklightMode::PermanentManual);
        assert_eq!(merged.effect, None);
        assert_eq!(merged.current_level, 6);
        assert_eq!(merged.duration_hands_out, 11);
        assert_eq!(merged.duration_hands_in, 22);
        assert_eq!(merged.duration_powered, 33);
    }
}
