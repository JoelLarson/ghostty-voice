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
}
