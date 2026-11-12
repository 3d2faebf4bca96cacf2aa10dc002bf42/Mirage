//! Key binding parsing.
//!
//! Names map to Windows virtual-key codes; the numeric form `0xNN` (or `vk:NN`)
//! is also accepted so a user can bind anything without us enumerating every key.

use anyhow::{anyhow, Result};

/// Parse a user-facing key name into a Windows virtual-key code.
///
/// Accepts `F13`, `space`, `ctrl`, `mouse4`, `0x77`, `vk:119`.
pub fn parse_key(name: &str) -> Result<u16> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("empty key name"));
    }

    // Explicit escapes first: they carry their own radix.
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u16::from_str_radix(hex, 16)
            .map_err(|error| anyhow!("invalid hex virtual-key `{name}`: {error}"));
    }
    if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("vk:") {
        return match rest.strip_prefix("0x") {
            Some(hex) => u16::from_str_radix(hex, 16)
                .map_err(|error| anyhow!("invalid virtual-key `{name}`: {error}")),
            None => rest
                .parse::<u16>()
                .map_err(|error| anyhow!("invalid virtual-key `{name}`: {error}")),
        };
    }

    // A single character means the key with that caption (`5` -> VK 0x35), not the
    // virtual-key value 5.
    if trimmed.len() == 1 {
        let byte = trimmed.as_bytes()[0];
        if byte.is_ascii_alphabetic() {
            return Ok(byte.to_ascii_uppercase() as u16);
        }
        if byte.is_ascii_digit() {
            return Ok(byte as u16);
        }
    }

    // Multi-digit bare numbers are treated as raw virtual-key codes.
    if let Ok(code) = trimmed.parse::<u16>() {
        return Ok(code);
    }

    let lowered = trimmed.to_ascii_lowercase();
    let code = match lowered.as_str() {
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "escape" | "esc" => 0x1B,
        "shift" => 0x10,
        "ctrl" | "control" => 0x11,
        "alt" | "menu" => 0x12,
        "capslock" => 0x14,
        "backspace" => 0x08,
        other if other.starts_with('f') => {
            let number: u16 = other[1..]
                .parse()
                .map_err(|_| anyhow!("unsupported key `{name}`"))?;
            if (1..=24).contains(&number) {
                0x70 + (number - 1) // VK_F1 = 0x70
            } else {
                return Err(anyhow!("function key out of range: `{name}`"));
            }
        }
        other if other.starts_with("mouse") => {
            let number: u16 = other[5..]
                .parse()
                .map_err(|_| anyhow!("unsupported mouse button `{name}`"))?;
            match number {
                // Windows virtual keys are not contiguous: the middle button sits at
                // 0x04 and the two extra buttons at 0x05/0x06.
                1 => 0x01, // VK_LBUTTON
                2 => 0x02, // VK_RBUTTON
                3 => 0x04, // VK_MBUTTON
                4 => 0x05, // VK_XBUTTON1
                5 => 0x06, // VK_XBUTTON2
                _ => return Err(anyhow!("mouse button out of range: `{name}`")),
            }
        }
        other => return Err(anyhow!("unsupported key `{other}`")),
    };
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_function_keys() {
        assert_eq!(parse_key("F1").unwrap(), 0x70);
        assert_eq!(parse_key("f13").unwrap(), 0x7C);
        assert_eq!(parse_key("F24").unwrap(), 0x87);
        assert!(parse_key("F25").is_err());
        assert!(parse_key("F0").is_err());
    }

    #[test]
    fn parses_letters_digits_and_named_keys() {
        assert_eq!(parse_key("a").unwrap(), 0x41);
        assert_eq!(parse_key("Z").unwrap(), 0x5A);
        assert_eq!(parse_key("5").unwrap(), 0x35);
        assert_eq!(parse_key("space").unwrap(), 0x20);
        assert_eq!(parse_key("Ctrl").unwrap(), 0x11);
        assert_eq!(parse_key("mouse4").unwrap(), 0x05);
    }

    #[test]
    fn parses_numeric_escapes() {
        assert_eq!(parse_key("0x77").unwrap(), 0x77);
        assert_eq!(parse_key("vk:119").unwrap(), 119);
        assert_eq!(parse_key("119").unwrap(), 119);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_key("").is_err());
        assert!(parse_key("not-a-key").is_err());
        assert!(parse_key("mouse9").is_err());
        assert!(parse_key("vk:zz").is_err());
    }
}
