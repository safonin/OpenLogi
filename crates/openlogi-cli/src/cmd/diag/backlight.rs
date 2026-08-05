//! `openlogi diag backlight` — read the HID++ `0x1982` backlight config and
//! info from a keyboard (e.g. MX Keys). Read-only diagnostic; pairs with
//! `diag lighting` (which writes solid RGB to G-series keyboards).

use anyhow::Result;
use clap::Args;
use openlogi_hid::{get_backlight_config, get_backlight_info};

use super::select_device;

/// The HID++ `Backlight2` feature id, used to pick a device that exposes it.
const BACKLIGHT_FEATURE: u16 = 0x1982;

#[derive(Debug, Args)]
pub struct BacklightArgs {
    /// Run against the device whose name contains this string (case-insensitive).
    #[arg(long, value_name = "NAME")]
    pub device: Option<String>,
}

pub async fn run(args: BacklightArgs) -> Result<()> {
    let (route, name) = select_device(args.device.as_deref(), &[BACKLIGHT_FEATURE]).await?;
    println!("device: {name} ({route})");

    match get_backlight_config(&route).await {
        Ok(cfg) => {
            println!("backlight config:");
            println!("  enabled:        {}", cfg.enabled);
            println!("  options:        {:?}", cfg.options);
            println!("  mode:           {:?}", cfg.mode);
            println!("  effect list:    {:?}", cfg.effect_list);
            println!("  current level:  {}", cfg.current_level);
            println!(
                "  fade-out (5s units): hands-out={} hands-in={} powered={}",
                cfg.duration_hands_out, cfg.duration_hands_in, cfg.duration_powered
            );
        }
        Err(e) => println!("  config read failed: {e:#}"),
    }

    match get_backlight_info(&route).await {
        Ok(info) => {
            println!("backlight info:");
            println!("  levels:         {}", info.nb_levels);
            println!("  current level:  {}", info.current_level);
            println!("  status:         {:?}", info.status);
            println!("  effect:         {:?}", info.effect);
            println!(
                "  oob fade-out (5s units): hands-out={} hands-in={} powered={}",
                info.oob_duration_hands_out, info.oob_duration_hands_in, info.oob_duration_powered
            );
        }
        Err(e) => println!("  info read failed: {e:#}"),
    }

    Ok(())
}
