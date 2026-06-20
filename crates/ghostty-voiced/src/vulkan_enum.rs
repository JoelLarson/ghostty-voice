//! Enumerate Vulkan devices by parsing `vulkaninfo`, so the configured PCI
//! address (ADR-0001) resolves to the enumeration index whisper.cpp uses for
//! `GGML_VK_VISIBLE_DEVICES`.
//!
//! NOTE: RADV encodes the PCI address inside `deviceUUID`
//! (`00000000-0300-…` ⇒ bus 0x03, device 0x00). This derivation, and the
//! assumption that vulkaninfo's enumeration order matches whisper's, need
//! validation on the real GPU.

use anyhow::{Context, Result};
use ghostty_voice_core::vulkan::{PciAddress, VulkanDevice};
use std::process::Command;

/// Parse `vulkaninfo` output into the ordered device list. Pairs each
/// `deviceName` with the following `deviceUUID`; the index is appearance order.
pub fn parse_vulkaninfo(output: &str) -> Vec<VulkanDevice> {
    let mut names = Vec::new();
    let mut uuids = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(v) = field(line, "deviceName") {
            names.push(v.to_owned());
        } else if let Some(v) = field(line, "deviceUUID") {
            uuids.push(v.to_owned());
        }
    }
    names
        .into_iter()
        .zip(uuids)
        .enumerate()
        .filter_map(|(i, (name, uuid))| {
            pci_from_uuid(&uuid).map(|pci| VulkanDevice {
                index: i as u32,
                name,
                pci_address: pci,
            })
        })
        .collect()
}

/// Run `vulkaninfo` and parse its device list.
pub fn enumerate() -> Result<Vec<VulkanDevice>> {
    let output = Command::new("vulkaninfo")
        .output()
        .context("failed to run vulkaninfo (is the Vulkan SDK installed?)")?;
    Ok(parse_vulkaninfo(&String::from_utf8_lossy(&output.stdout)))
}

/// Extract the value of a `key  = value` vulkaninfo line.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.trim_start();
    Some(rest.strip_prefix('=')?.trim())
}

/// Derive the PCI address from a RADV `deviceUUID` (group 2 is `BBDD`).
fn pci_from_uuid(uuid: &str) -> Option<PciAddress> {
    let group = uuid.split('-').nth(1)?;
    if group.len() < 4 {
        return None;
    }
    Some(PciAddress {
        domain: 0,
        bus: u8::from_str_radix(&group[0..2], 16).ok()?,
        device: u8::from_str_radix(&group[2..4], 16).ok()?,
        function: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostty_voice_core::vulkan::resolve_device_index;

    const SAMPLE: &str = "
\tdeviceName        = AMD Radeon RX 6900 XT (RADV NAVI21)
\tdriverUUID        = 414d442d-4d45-5341-2d44-525600000000
\tdeviceUUID                        = 00000000-0300-0000-0000-000000000000
\tdeviceName        = AMD Ryzen 9 7950X 16-Core Processor (RADV RAPHAEL_MENDOCINO)
\tdeviceUUID                        = 00000000-1a00-0000-0000-000000000000
";

    #[test]
    fn parses_devices_with_index_name_and_pci() {
        let devices = parse_vulkaninfo(SAMPLE);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].index, 0);
        assert!(devices[0].name.contains("RX 6900 XT"));
        assert_eq!(
            devices[0].pci_address,
            PciAddress::parse("0000:03:00.0").unwrap()
        );
        assert_eq!(
            devices[1].pci_address,
            PciAddress::parse("0000:1a:00.0").unwrap()
        );
    }

    #[test]
    fn resolves_configured_pci_to_the_discrete_index() {
        let devices = parse_vulkaninfo(SAMPLE);
        let target = PciAddress::parse("0000:03:00.0").unwrap();
        assert_eq!(resolve_device_index(&devices, target), Ok(0));
    }

    #[test]
    fn driver_uuid_is_not_mistaken_for_device_uuid() {
        // Only deviceUUID lines should be consumed, not driverUUID.
        let devices = parse_vulkaninfo(SAMPLE);
        assert_eq!(devices.len(), 2);
    }
}
