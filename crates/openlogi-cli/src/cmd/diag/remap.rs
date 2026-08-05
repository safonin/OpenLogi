//! `openlogi diag remap <CID> [TARGET]` — read, set, and reset a HID++ 0x1b04
//! reprogrammable control's remap. Read-only when no target is given; with a
//! target it writes the remap, reads it back, then resets to factory default.

use anyhow::{Context, Result, bail};
use clap::Args;
use openlogi_hid::reprog_controls::{CidReporting, CidReportingChange, ControlId};
use openlogi_hid::{get_cid_reporting, set_cid_reporting};

use crate::cmd::diag::select_device;

/// The HID++ `ReprogControls` feature id, used to pick a device that exposes it.
const REPROG_FEATURE: u16 = 0x1b04;

#[derive(Debug, Args)]
pub struct RemapArgs {
    /// Control ID to read/remap, as hex (e.g. `0x00e4` for the Prev Track key).
    pub cid: String,
    /// Target control ID to remap to, as hex. Omit for a read-only dump.
    pub target: Option<String>,
    /// Run against the device whose name contains this string (case-insensitive).
    #[arg(long, value_name = "NAME")]
    pub device: Option<String>,
}

fn parse_cid(s: &str) -> Result<ControlId> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    let value = u16::from_str_radix(s, 16).context("parse hex control id")?;
    Ok(ControlId(value))
}

pub async fn run(args: RemapArgs) -> Result<()> {
    let cid = parse_cid(&args.cid)?;
    let (route, name) = select_device(args.device.as_deref(), &[REPROG_FEATURE]).await?;
    println!("device: {name} ({route})");

    // Read current state.
    let before = get_cid_reporting(&route, cid)
        .await
        .context("read cid reporting")?;
    print_reporting("before", &before);

    let Some(target_str) = args.target.as_deref() else {
        return Ok(());
    };
    let target = parse_cid(target_str)?;

    if target.0 == 0 {
        bail!("target control id 0x0000 is the no-remap sentinel, not a valid target");
    }

    // Write the remap.
    let change = CidReportingChange {
        remap: Some(target),
        ..Default::default()
    };
    set_cid_reporting(&route, cid, change)
        .await
        .context("write cid reporting (remap)")?;
    println!("\nremap written: 0x{:04x} -> 0x{:04x}", cid.0, target.0);

    // Read back.
    match get_cid_reporting(&route, cid).await {
        Ok(after) => print_reporting("after", &after),
        Err(e) => println!("\n  read-back failed: {e:#}"),
    }

    // Reset to factory default (remap = 0 = no remapping).
    let reset = CidReportingChange {
        remap: Some(ControlId(0)),
        ..Default::default()
    };
    match set_cid_reporting(&route, cid, reset).await {
        Ok(()) => println!("\nreset to factory default (remap cleared)"),
        Err(e) => println!("\nreset failed: {e:#} — the control may still be remapped!"),
    }
    Ok(())
}

fn print_reporting(label: &str, rep: &CidReporting) {
    println!(
        "\n{label}: cid=0x{:04x} diverted={} persistently_diverted={} remap={}",
        rep.cid.0,
        rep.diverted,
        rep.persistently_diverted,
        match rep.remap {
            Some(t) => format!("0x{:04x}", t.0),
            None => "none".to_string(),
        }
    );
}
