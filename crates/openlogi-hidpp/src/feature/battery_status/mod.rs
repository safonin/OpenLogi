//! Implements the `BatteryStatus` feature (ID `0x1000`) — the older HID++ 2.0
//! battery query used by devices that predate `UnifiedBattery` (`0x1004`),
//! such as the Logitech MX Keys.
//!
//! Unlike `0x1004`, this feature reports a coarse battery level (a percentage
//! plus a "next level" transition point) and a charging status, but not an
//! explicit charge-state vocabulary as rich as UnifiedBattery's.

use std::sync::Arc;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    channel::HidppChannel,
    feature::{CreatableFeature, Feature, FeatureEndpoint},
    protocol::v20::Hidpp20Error,
};

/// Battery status reported by [`BatteryStatusFeature::get_battery_level_status`].
///
/// Values follow the HID++ 2.0 `0x1000` specification (and the Linux
/// `hid-logitech-hidpp` driver / Solaar): byte 2 of the
/// `GetBatteryLevelStatus` response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
#[repr(u8)]
pub enum BatteryStatus {
    /// Running on battery, discharging.
    Discharging = 0x00,
    /// Charging (recharging).
    Recharging = 0x01,
    /// Charge almost complete / full while on external power.
    ChargeComplete = 0x02,
    /// Charging at a reduced current (slow recharge).
    SlowRecharge = 0x03,
    /// Invalid battery / not present.
    InvalidBattery = 0x04,
    /// Thermal error.
    ThermalError = 0x05,
    /// Other error, not charging.
    Error = 0x06,
}

/// The battery level status from [`BatteryStatusFeature::get_battery_level_status`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct BatteryLevelStatus {
    /// Current battery discharge level as a percentage (`0..=100`). `0` is a
    /// valid "critically low" reading, not a sentinel.
    pub current_level: u8,
    /// The next lower level the device will report as charge decreases, as a
    /// percentage. Useful for UI to anticipate the next transition.
    pub next_level: u8,
    /// Charging status.
    pub status: BatteryStatus,
}

/// Implements the `BatteryStatus` / `0x1000` feature.
#[derive(Clone)]
pub struct BatteryStatusFeature {
    /// The endpoint this feature talks to.
    endpoint: FeatureEndpoint,
}

impl CreatableFeature for BatteryStatusFeature {
    const ID: u16 = 0x1000;
    /// Bound from version 1: function 0 is `GetBatteryLevelStatus` (the
    /// percentage / next-level / status form) from v1 onward. A v0 device would
    /// report a different layout under function 0, so binding at 1 avoids
    /// decoding a v0 response with v1 semantics.
    const STARTING_VERSION: u8 = 1;

    fn new(chan: Arc<HidppChannel>, device_index: u8, feature_index: u8) -> Self {
        Self {
            endpoint: FeatureEndpoint::new(chan, device_index, feature_index),
        }
    }
}

impl Feature for BatteryStatusFeature {}

impl BatteryStatusFeature {
    /// Retrieves the current battery level, the next-lower level, and the
    /// charging status (`GetBatteryLevelStatus`, function 0).
    pub async fn get_battery_level_status(&self) -> Result<BatteryLevelStatus, Hidpp20Error> {
        let payload = self.endpoint.call(0, [0; 3]).await?.extend_payload();
        Ok(BatteryLevelStatus {
            current_level: payload[0],
            next_level: payload[1],
            status: BatteryStatus::try_from(payload[2])
                .map_err(|_| Hidpp20Error::UnsupportedResponse)?,
        })
    }

    /// Retrieves the number of supported battery levels
    /// (`GetBatteryCapability`, function 1).
    pub async fn get_battery_capability(&self) -> Result<u8, Hidpp20Error> {
        let payload = self.endpoint.call(1, [0; 3]).await?.extend_payload();
        Ok(payload[0])
    }
}

#[cfg(test)]
mod tests {
    use super::BatteryStatus;

    #[test]
    fn decodes_known_status_values() {
        assert_eq!(
            BatteryStatus::try_from(0x00).unwrap(),
            BatteryStatus::Discharging
        );
        assert_eq!(
            BatteryStatus::try_from(0x01).unwrap(),
            BatteryStatus::Recharging
        );
        assert_eq!(
            BatteryStatus::try_from(0x03).unwrap(),
            BatteryStatus::SlowRecharge
        );
    }

    #[test]
    fn rejects_unknown_status_value() {
        assert!(BatteryStatus::try_from(0xff).is_err());
    }
}
