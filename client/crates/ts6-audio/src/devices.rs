//! Device enumeration (input and output), used by the CLI and later by the UI.

use cpal::traits::{DeviceTrait, HostTrait};

/// A user-selectable audio device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_format: String,
}

impl std::fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{} — {} ch @ {} Hz, {}",
            if self.is_default { "* " } else { "  " },
            self.name,
            self.channels,
            self.sample_rate,
            self.sample_format
        )
    }
}

/// cpal 0.18 exposes `description().name()`; older versions had `name()`.
pub(crate) fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| "<unnamed>".to_string())
}

fn describe(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    is_default: bool,
) -> DeviceInfo {
    DeviceInfo {
        name: device_name(device),
        is_default,
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        sample_format: format!("{:?}", config.sample_format()),
    }
}

pub fn inputs() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().map(|d| device_name(&d));
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let config = device.default_input_config().ok()?;
            let name = device_name(&device);
            Some(describe(
                &device,
                &config,
                default_name.as_deref() == Some(name.as_str()),
            ))
        })
        .collect()
}

pub fn outputs() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default_name = host.default_output_device().map(|d| device_name(&d));
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let config = device.default_output_config().ok()?;
            let name = device_name(&device);
            Some(describe(
                &device,
                &config,
                default_name.as_deref() == Some(name.as_str()),
            ))
        })
        .collect()
}

/// Resolve a device by exact name, or fall back to the host default.
pub(crate) fn pick_input(device: Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    match device {
        Some(wanted) => host
            .input_devices()
            .ok()?
            .into_iter()
            .find(|d| device_name(d) == wanted)
            .or_else(|| host.default_input_device()),
        None => host.default_input_device(),
    }
}

pub(crate) fn pick_output(device: Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    match device {
        Some(wanted) => host
            .output_devices()
            .ok()?
            .into_iter()
            .find(|d| device_name(d) == wanted)
            .or_else(|| host.default_output_device()),
        None => host.default_output_device(),
    }
}
