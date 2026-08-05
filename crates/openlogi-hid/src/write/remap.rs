//! HID++ `ReprogControls` (`0x1b04`) — read and write a control's remap state.
//!
//! `setCidReporting` with a `remap` target rewrites what HID usage a
//! reprogrammable control emits, persistently across reconnects on devices that
//! advertise `PERSISTENTLY_DIVERTABLE` / `REPROGRAMMABLE` controls. The HID++
//! feature itself is implemented in `openlogi-hidpp`; this module is the
//! route-resolved entry point the agent and CLI use, mirroring
//! [`super::dpi`] / [`super::lighting`].

use std::sync::Arc;

use hidpp::{
    device::Device,
    feature::{
        CreatableFeature,
        reprog_controls::{CidReporting, CidReportingChange, ControlId, ReprogControlsFeature},
    },
};
use openlogi_core::config::KeyRemap;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::route::DeviceRoute;

use super::{HidppOperation, WriteError, classify_hidpp_error, open_feature, with_route};

/// Read the current reporting/remap state for `cid` on the device at `route`.
pub async fn get_cid_reporting(
    route: &DeviceRoute,
    cid: ControlId,
) -> Result<CidReporting, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<ReprogControlsFeature>(&mut device).await?;
        feature.get_cid_reporting(cid).await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadRemap, ReprogControlsFeature::ID)
        })
    })
    .await
}

/// Apply a reporting/remap change to `cid` on the device at `route`.
///
/// `change` carries only the fields to modify; `None` fields are left unchanged
/// by the device (each maps to a `*-valid` bit in the HID++ packet).
pub async fn set_cid_reporting(
    route: &DeviceRoute,
    cid: ControlId,
    change: CidReportingChange,
) -> Result<(), WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<ReprogControlsFeature>(&mut device).await?;
        feature.set_cid_reporting(cid, change).await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::WriteRemap, ReprogControlsFeature::ID)
        })?;
        debug!(index, ?cid, "wrote cid reporting");
        Ok(())
    })
    .await
}

/// Replace the device's full key-remap map. Existing remaps whose source is no
/// longer present are explicitly reset with target `0`; desired mappings are
/// then applied. The remap is volatile, so the agent re-applies on reconnect.
///
/// Returns `Ok(())` once every remap is written; a per-key failure
/// short-circuits with the first error.
pub async fn apply_key_remap(route: &DeviceRoute, remap: &KeyRemap) -> Result<(), WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<ReprogControlsFeature>(&mut device).await?;
        let count = feature.get_count().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadRemap, ReprogControlsFeature::ID)
        })?;
        let mut current = Vec::with_capacity(count as usize);
        for position in 0..count {
            let info = feature.get_cid_info(position).await.map_err(|e| {
                classify_hidpp_error(e, HidppOperation::ReadRemap, ReprogControlsFeature::ID)
            })?;
            let reporting = feature.get_cid_reporting(info.cid).await.map_err(|e| {
                classify_hidpp_error(e, HidppOperation::ReadRemap, ReprogControlsFeature::ID)
            })?;
            current.push((info.cid.0, reporting.remap.map(|target| target.0)));
        }

        for (source, target) in replacement_changes(&current, remap) {
            feature
                .set_cid_reporting(
                    ControlId(source),
                    CidReportingChange {
                        remap: target.map(ControlId),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| {
                    classify_hidpp_error(e, HidppOperation::WriteRemap, ReprogControlsFeature::ID)
                })?;
        }
        Ok(())
    })
    .await
}

fn replacement_changes(
    current: &[(u16, Option<u16>)],
    desired: &KeyRemap,
) -> Vec<(u16, Option<u16>)> {
    let mut changes = current
        .iter()
        .filter_map(|&(source, target)| {
            (target.is_some() && !desired.0.contains_key(&source)).then_some((source, None))
        })
        .collect::<Vec<_>>();
    changes.extend(
        desired
            .0
            .iter()
            .map(|(&source, &target)| (source, Some(target))),
    );
    changes
}

/// One reprogrammable control, in the IPC-safe form that crosses the agent↔GUI
/// wire. Unlike the hidpp `CidInfo` (which is not `Serialize` under the
/// workspace's feature set), this is a plain serializable snapshot of the
/// fields the remap UI needs.
///
/// Field order is wire format — changes require a `PROTOCOL_VERSION` bump
/// (guarded by `openlogi-agent-core/tests/wire_format.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemappableControl {
    /// HID++ control ID (the source key, e.g. `0x00e4` for Prev Track).
    pub cid: u16,
    /// Default task ID the control is bound to.
    pub task_id: u16,
    /// Whether the control can be diverted/remapped by software.
    pub divertable: bool,
}

/// Read every reprogrammable control the device at `route` exposes, in table
/// order. Used by the GUI's key-remap panel to list the divertable keys.
pub async fn read_remappable_controls(
    route: &DeviceRoute,
) -> Result<Vec<RemappableControl>, WriteError> {
    let index = route.device_index();
    with_route(route, move |channel| async move {
        let mut device = Device::new(Arc::clone(&channel), index)
            .await
            .map_err(|_| WriteError::DeviceUnreachable { index })?;
        let feature = open_feature::<ReprogControlsFeature>(&mut device).await?;
        let count = feature.get_count().await.map_err(|e| {
            classify_hidpp_error(e, HidppOperation::ReadRemap, ReprogControlsFeature::ID)
        })?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let info = feature.get_cid_info(i).await.map_err(|e| {
                classify_hidpp_error(e, HidppOperation::ReadRemap, ReprogControlsFeature::ID)
            })?;
            out.push(RemappableControl {
                cid: info.cid.into(),
                task_id: info.task_id.into(),
                divertable: info.flags.is_divertable(),
            });
        }
        Ok(out)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::replacement_changes;
    use openlogi_core::config::KeyRemap;

    #[test]
    fn replacement_resets_removed_sources_before_applying_desired_map() {
        let current = [(0x00e4, Some(0x00e5)), (0x00e6, Some(0x00e7))];
        let mut desired = KeyRemap::default();
        desired.0.insert(0x00e6, 0x00e8);
        assert_eq!(
            replacement_changes(&current, &desired),
            vec![(0x00e4, None), (0x00e6, Some(0x00e8))]
        );
    }

    #[test]
    fn empty_replacement_resets_every_current_remap() {
        let current = [(0x00e4, Some(0x00e5)), (0x00e6, None)];
        assert_eq!(
            replacement_changes(&current, &KeyRemap::default()),
            vec![(0x00e4, None)]
        );
    }
}
