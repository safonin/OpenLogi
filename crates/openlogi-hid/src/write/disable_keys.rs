//! HID++ `DisableKeys` (`0x4521`) — disable a fixed set of lock/system keys.
//!
//! Reads capabilities and the current disabled set, and writes a replacement
//! set. The HID++ feature is implemented in `openlogi-hidpp`; this module is
//! the route-resolved entry point, mirroring [`super::dpi`] / [`super::lighting`].

use std::sync::Arc;

use hidpp::{
    device::Device,
    feature::{
        CreatableFeature,
        disable_keys::{DisableKeysFeature, DisableableKeys},
    },
};
use openlogi_core::config::DisabledKeys;
use tracing::debug;

use crate::route::DeviceRoute;

use super::{HidppOperation, WriteError, classify_hidpp_error, open_feature, with_route};

/// Read the set of keys the device allows software to disable.
pub async fn get_disable_keys_capabilities(
    route: &DeviceRoute,
) -> Result<DisableableKeys, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<DisableKeysFeature>(&mut device).await?;
        feature.get_capabilities().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadDisableKeys, DisableKeysFeature::ID)
        })
    })
    .await
}

/// Read the set of keys currently disabled.
pub async fn get_disabled_keys(route: &DeviceRoute) -> Result<DisabledKeys, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<DisableKeysFeature>(&mut device).await?;
        let disabled = feature.get_disabled_keys().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadDisableKeys, DisableKeysFeature::ID)
        })?;
        Ok(disabled_keys_from_hidpp(disabled))
    })
    .await
}

/// Replace the set of disabled keys. Passing an all-false set re-enables every key.
pub async fn set_disabled_keys(route: &DeviceRoute, keys: DisabledKeys) -> Result<(), WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<DisableKeysFeature>(&mut device).await?;
        let supported = feature.get_capabilities().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadDisableKeys, DisableKeysFeature::ID)
        })?;
        let effective = mask_disabled_keys(keys, supported);
        feature.set_disabled_keys(effective).await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::WriteDisableKeys, DisableKeysFeature::ID)
        })?;
        debug!(index, ?keys, ?supported, ?effective, "wrote disabled keys");
        Ok(())
    })
    .await
}

fn mask_disabled_keys(keys: DisabledKeys, supported: DisableableKeys) -> DisableableKeys {
    disabled_keys_to_hidpp(keys) & supported
}

/// Map the IPC-safe [`DisabledKeys`] onto the HID++ `DisableableKeys` bitflags.
fn disabled_keys_to_hidpp(keys: DisabledKeys) -> DisableableKeys {
    let mut bits = DisableableKeys::empty();
    if keys.caps_lock {
        bits |= DisableableKeys::CAPS_LOCK;
    }
    if keys.num_lock {
        bits |= DisableableKeys::NUM_LOCK;
    }
    if keys.scroll_lock {
        bits |= DisableableKeys::SCROLL_LOCK;
    }
    if keys.insert {
        bits |= DisableableKeys::INSERT;
    }
    if keys.windows {
        bits |= DisableableKeys::WINDOWS;
    }
    bits
}

/// Map the HID++ `DisableableKeys` bitflags onto the IPC-safe [`DisabledKeys`].
fn disabled_keys_from_hidpp(bits: DisableableKeys) -> DisabledKeys {
    DisabledKeys {
        caps_lock: bits.contains(DisableableKeys::CAPS_LOCK),
        num_lock: bits.contains(DisableableKeys::NUM_LOCK),
        scroll_lock: bits.contains(DisableableKeys::SCROLL_LOCK),
        insert: bits.contains(DisableableKeys::INSERT),
        windows: bits.contains(DisableableKeys::WINDOWS),
    }
}

#[cfg(test)]
mod tests {
    use super::mask_disabled_keys;
    use hidpp::feature::disable_keys::DisableableKeys;
    use openlogi_core::config::DisabledKeys;

    #[test]
    fn write_masks_keys_the_device_does_not_support() {
        let requested = DisabledKeys {
            caps_lock: true,
            num_lock: true,
            scroll_lock: true,
            insert: true,
            windows: true,
        };
        let supported = DisableableKeys::CAPS_LOCK | DisableableKeys::INSERT;
        assert_eq!(mask_disabled_keys(requested, supported), supported);
    }
}
