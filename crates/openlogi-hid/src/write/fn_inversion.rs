//! HID++ Fn-inversion (Fn-lock) — `0x40a3` (multi-host) / `0x40a2`.
//!
//! Reads and writes the global function-key inversion state (Fn-lock). The HID++
//! feature is implemented in `openlogi-hidpp`; this module is the route-resolved
//! entry point, mirroring [`super::dpi`] / [`super::lighting`].
//!
//! Two feature variants exist: `0x40a3` (multi-host, per-slot state) and `0x40a2`
//! (single-host, global state). The multi-host feature is preferred; the
//! single-host feature is the fallback for devices that predate it.

use std::sync::Arc;

use hidpp::{
    device::Device,
    feature::{
        CreatableFeature,
        fn_inversion::{
            FnInversionMultiHostFeature, FnInversionState, FnInversionWithDefaultStateFeature,
        },
        hosts_info::HostIndex,
    },
};
use openlogi_core::config::FnLock;
use tracing::debug;

use crate::route::DeviceRoute;

use super::{HidppOperation, WriteError, classify_hidpp_error, open_feature, with_route};

/// Read the current Fn-lock state for the current host slot on the device at
/// `route`. Prefers the multi-host feature (`0x40a3`), falling back to the
/// single-host feature (`0x40a2`) when the device exposes only that.
pub async fn get_fn_inversion(route: &DeviceRoute) -> Result<FnLock, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        // Prefer the multi-host feature (0x40a3); fall back to single-host (0x40a2).
        match open_feature::<FnInversionMultiHostFeature>(&mut device).await {
            Ok(feature) => {
                let info = feature
                    .get_global_fn_inversion(HostIndex::Current)
                    .await
                    .map_err(|e| {
                        classify_hidpp_error(
                            e,
                            HidppOperation::ReadFnInversion,
                            FnInversionMultiHostFeature::ID,
                        )
                    })?;
                return Ok(FnLock {
                    enabled: info.state == FnInversionState::On,
                });
            }
            Err(error) if multi_host_is_unsupported(&error) => {}
            Err(error) => return Err(error),
        }
        let feature = open_feature::<FnInversionWithDefaultStateFeature>(&mut device).await?;
        let info = feature.get_global_fn_inversion().await.map_err(|e| {
            classify_hidpp_error(
                e,
                HidppOperation::ReadFnInversion,
                FnInversionWithDefaultStateFeature::ID,
            )
        })?;
        Ok(FnLock {
            enabled: info.state == FnInversionState::On,
        })
    })
    .await
}

/// Write the Fn-lock state for the current host slot on the device at `route`.
/// Prefers the multi-host feature (`0x40a3`), falling back to the single-host
/// feature (`0x40a2`) when the device exposes only that.
pub async fn set_fn_inversion(route: &DeviceRoute, lock: FnLock) -> Result<(), WriteError> {
    let state = if lock.enabled {
        FnInversionState::On
    } else {
        FnInversionState::Off
    };
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        // Prefer the multi-host feature (0x40a3); fall back to single-host (0x40a2).
        match open_feature::<FnInversionMultiHostFeature>(&mut device).await {
            Ok(feature) => {
                feature
                    .set_global_fn_inversion(HostIndex::Current, state)
                    .await
                    .map_err(|e| {
                        classify_hidpp_error(
                            e,
                            HidppOperation::WriteFnInversion,
                            FnInversionMultiHostFeature::ID,
                        )
                    })?;
                debug!(
                    index,
                    enabled = lock.enabled,
                    "wrote fn inversion (multi-host)"
                );
                return Ok(());
            }
            Err(error) if multi_host_is_unsupported(&error) => {}
            Err(error) => return Err(error),
        }
        let feature = open_feature::<FnInversionWithDefaultStateFeature>(&mut device).await?;
        feature.set_global_fn_inversion(state).await.map_err(|e| {
            classify_hidpp_error(
                e,
                HidppOperation::WriteFnInversion,
                FnInversionWithDefaultStateFeature::ID,
            )
        })?;
        debug!(
            index,
            enabled = lock.enabled,
            "wrote fn inversion (single-host)"
        );
        Ok(())
    })
    .await
}

fn multi_host_is_unsupported(error: &WriteError) -> bool {
    matches!(
        error,
        WriteError::FeatureUnsupported { feature_hex }
            if *feature_hex == FnInversionMultiHostFeature::ID
    )
}

#[cfg(test)]
mod tests {
    use super::multi_host_is_unsupported;
    use crate::{HidppOperation, WriteError};
    use hidpp::feature::{CreatableFeature as _, fn_inversion::FnInversionMultiHostFeature};

    #[test]
    fn single_host_fallback_is_only_for_missing_multi_host_feature() {
        assert!(multi_host_is_unsupported(&WriteError::FeatureUnsupported {
            feature_hex: FnInversionMultiHostFeature::ID,
        }));
        assert!(!multi_host_is_unsupported(&WriteError::RequestTimedOut {
            operation: HidppOperation::ResolveFeature,
        }));
        assert!(!multi_host_is_unsupported(&WriteError::DeviceUnreachable {
            index: 1,
        }));
    }
}
