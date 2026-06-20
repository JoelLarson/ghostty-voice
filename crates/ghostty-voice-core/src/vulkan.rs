//! GPU device pinning (ADR-0001).
//!
//! This workstation exposes two RADV devices (the discrete RX 6900 XT and the
//! integrated Raphael GPU). whisper.cpp selects a device by *enumeration index*
//! (`GGML_VK_VISIBLE_DEVICES`) and will silently run on the wrong one if not
//! pinned. We therefore resolve a configured **PCI address** to that index and
//! assert the loaded device's name. This module is the pure logic for both.

/// A PCI address such as `0000:03:00.0` — `domain:bus:device.function`,
/// each field hexadecimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciAddress {
    pub domain: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

/// Why a PCI address string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PciParseError {
    /// The string was not a well-formed `domain:bus:device.function` address.
    Malformed(String),
}

impl PciAddress {
    /// Parse a `domain:bus:device.function` address such as `0000:03:00.0`.
    pub fn parse(s: &str) -> Result<Self, PciParseError> {
        fn malformed(s: &str) -> PciParseError {
            PciParseError::Malformed(s.to_owned())
        }

        let (head, function) = s.rsplit_once('.').ok_or_else(|| malformed(s))?;
        let mut fields = head.split(':');
        let domain = fields.next().ok_or_else(|| malformed(s))?;
        let bus = fields.next().ok_or_else(|| malformed(s))?;
        let device = fields.next().ok_or_else(|| malformed(s))?;
        if fields.next().is_some() {
            return Err(malformed(s));
        }

        Ok(PciAddress {
            domain: u16::from_str_radix(domain, 16).map_err(|_| malformed(s))?,
            bus: u8::from_str_radix(bus, 16).map_err(|_| malformed(s))?,
            device: u8::from_str_radix(device, 16).map_err(|_| malformed(s))?,
            function: u8::from_str_radix(function, 16).map_err(|_| malformed(s))?,
        })
    }
}

/// One Vulkan device as enumerated by the driver. `index` is the value
/// `GGML_VK_VISIBLE_DEVICES` uses to select it; `pci_address` is obtained at
/// the enumeration boundary (e.g. via `VK_EXT_pci_bus_info`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulkanDevice {
    pub index: u32,
    pub name: String,
    pub pci_address: PciAddress,
}

/// Why a configured PCI address could not be resolved to a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No enumerated device sits at the target PCI address.
    DeviceNotFound { target: PciAddress },
    /// More than one device claims the target address (should not happen).
    AmbiguousMatch { target: PciAddress, count: usize },
}

/// Resolve a configured PCI address to the Vulkan enumeration index to pin.
pub fn resolve_device_index(
    devices: &[VulkanDevice],
    target: PciAddress,
) -> Result<u32, ResolveError> {
    let matches: Vec<&VulkanDevice> = devices.iter().filter(|d| d.pci_address == target).collect();
    match matches.as_slice() {
        [] => Err(ResolveError::DeviceNotFound { target }),
        [only] => Ok(only.index),
        many => Err(ResolveError::AmbiguousMatch {
            target,
            count: many.len(),
        }),
    }
}

/// The loaded device's reported name did not contain the expected fragment —
/// likely the wrong GPU was selected (ADR-0001), so we refuse to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceNameMismatch {
    pub expected_contains: String,
    pub loaded_name: String,
}

/// Verify that the loaded device's reported name contains the expected
/// fragment (case-insensitive) — the backstop against silently pinning the
/// wrong GPU even after resolving an index.
pub fn verify_device_name(
    loaded_name: &str,
    expected_contains: &str,
) -> Result<(), DeviceNameMismatch> {
    if loaded_name
        .to_lowercase()
        .contains(&expected_contains.to_lowercase())
    {
        Ok(())
    } else {
        Err(DeviceNameMismatch {
            expected_contains: expected_contains.to_owned(),
            loaded_name: loaded_name.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_pci_address() {
        let addr = PciAddress::parse("0000:03:00.0").expect("should parse");
        assert_eq!(
            addr,
            PciAddress {
                domain: 0x0000,
                bus: 0x03,
                device: 0x00,
                function: 0,
            }
        );
    }

    #[test]
    fn parses_hex_fields() {
        // The iGPU sits at 1a:00.0 — bus is hex, not decimal.
        let addr = PciAddress::parse("0000:1a:00.0").expect("should parse");
        assert_eq!(addr.bus, 0x1a);
    }

    #[test]
    fn rejects_garbage() {
        assert!(PciAddress::parse("not-a-pci-address").is_err());
    }

    fn pci(s: &str) -> PciAddress {
        PciAddress::parse(s).expect("test fixture address should parse")
    }

    /// The real two-RADV-device layout on the dev workstation (ADR-0001).
    fn sample_devices() -> Vec<VulkanDevice> {
        vec![
            VulkanDevice {
                index: 0,
                name: "AMD Radeon RX 6900 XT (RADV NAVI21)".to_owned(),
                pci_address: pci("0000:03:00.0"),
            },
            VulkanDevice {
                index: 1,
                name: "AMD Ryzen 9 7950X 16-Core Processor (RADV RAPHAEL_MENDOCINO)".to_owned(),
                pci_address: pci("0000:1a:00.0"),
            },
        ]
    }

    #[test]
    fn resolves_discrete_gpu_to_its_index() {
        let idx = resolve_device_index(&sample_devices(), pci("0000:03:00.0")).unwrap();
        assert_eq!(idx, 0);
    }

    #[test]
    fn resolves_igpu_to_its_index() {
        let idx = resolve_device_index(&sample_devices(), pci("0000:1a:00.0")).unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn missing_address_is_device_not_found() {
        let target = pci("0000:0a:00.0");
        assert_eq!(
            resolve_device_index(&sample_devices(), target),
            Err(ResolveError::DeviceNotFound { target }),
        );
    }

    #[test]
    fn duplicate_address_is_ambiguous() {
        // The enumeration boundary should never produce this; if it does
        // (e.g. a parsing bug), fail loudly rather than silently pick one.
        let target = pci("0000:03:00.0");
        let devices = vec![
            VulkanDevice {
                index: 0,
                name: "first".to_owned(),
                pci_address: target,
            },
            VulkanDevice {
                index: 1,
                name: "second".to_owned(),
                pci_address: target,
            },
        ];
        assert_eq!(
            resolve_device_index(&devices, target),
            Err(ResolveError::AmbiguousMatch { target, count: 2 }),
        );
    }

    const DISCRETE: &str = "AMD Radeon RX 6900 XT (RADV NAVI21)";
    const IGPU: &str = "AMD Ryzen 9 7950X 16-Core Processor (RADV RAPHAEL_MENDOCINO)";

    #[test]
    fn accepts_matching_device_name() {
        assert!(verify_device_name(DISCRETE, "RX 6900 XT").is_ok());
    }

    #[test]
    fn name_match_is_case_insensitive() {
        assert!(verify_device_name(DISCRETE, "rx 6900 xt").is_ok());
    }

    #[test]
    fn rejects_wrong_device() {
        let err = verify_device_name(IGPU, "RX 6900 XT").unwrap_err();
        assert_eq!(
            err,
            DeviceNameMismatch {
                expected_contains: "RX 6900 XT".to_owned(),
                loaded_name: IGPU.to_owned(),
            }
        );
    }
}
