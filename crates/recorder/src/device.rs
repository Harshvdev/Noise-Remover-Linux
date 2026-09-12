//! Audio input device enumeration, filtering, and configuration inspection.

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host};
use crate::error::RecorderError;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: usize,
    pub name: String,
    pub default_sample_rate: u32,
    pub default_channels: u16,
    pub supported_sample_rates: Vec<u32>,
    pub supported_channels: Vec<u16>,
}

pub struct DeviceManager {
    host: Host,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }

    /// Check if a raw ALSA device description is an internal plugin or dummy device.
    fn is_redundant_device(name: &str) -> bool {
        let lower = name.to_lowercase();
        lower.starts_with("discard all samples")
            || lower.contains("rate converter")
            || lower.contains("jack audio")
            || lower.contains("open sound system")
            || lower.contains("plugin using")
            || lower.contains("plugin for")
            || lower.contains("surround")
            || lower.contains("dsnoop")
            || lower.contains("dmix")
            || lower.starts_with("pipewire sound server") // redundant with Default
            || lower.starts_with("pulseaudio sound server") // redundant with Default
    }

    /// On Linux, query the sound server (PipeWire/PulseAudio) to get the human-readable
    /// description of the currently active default input source (e.g. "Rockerz 411").
    pub fn get_system_default_source_description() -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("pactl")
                .args(["get-default-source"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let default_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if default_name.is_empty() {
                return None;
            }

            let list_out = Command::new("pactl")
                .args(["list", "sources"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&list_out.stdout);
            let mut cur_name = "";
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(n) = trimmed.strip_prefix("Name: ") {
                    cur_name = n.trim();
                } else if let Some(desc) = trimmed.strip_prefix("Description: ") {
                    if cur_name == default_name {
                        return Some(desc.trim().to_string());
                    }
                }
            }
        }
        None
    }

    /// Format a friendly human-readable name for audio devices.
    fn format_friendly_name(name: &str) -> String {
        let lower = name.to_lowercase();
        if lower.contains("default") || lower == "pipewire" || lower == "pulse" {
            if let Some(active) = Self::get_system_default_source_description() {
                return format!("Default ({})", active);
            }
            return "Default (System Preferred)".to_string();
        }
        if lower.contains("alc257") || lower.contains("generic") {
            return "Built-in Microphone (ALC257 Analog)".to_string();
        }
        name.to_string()
    }

    /// List cleaned and curated available input devices.
    /// Filters out internal ALSA virtual multi-channel routing plugins and ensures
    /// "Default (System Preferred)" appears first.
    pub fn list_input_devices(&self) -> Result<Vec<(usize, String)>, RecorderError> {
        let devices = self.host.input_devices()?;
        let mut curated = Vec::new();
        let mut default_entry = None;

        for (idx, dev) in devices.enumerate() {
            let raw_name = dev
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| format!("Device #{}", idx));

            if Self::is_redundant_device(&raw_name) {
                continue;
            }

            let friendly_name = Self::format_friendly_name(&raw_name);

            if friendly_name == "Default (System Preferred)" {
                if default_entry.is_none() {
                    default_entry = Some((idx, friendly_name));
                }
                continue;
            }

            // Avoid duplicate display names
            if !curated.iter().any(|(_, n): &(usize, String)| *n == friendly_name) {
                curated.push((idx, friendly_name));
            }
        }

        // Place the default system device at the very top
        let mut result = Vec::new();
        if let Some(def) = default_entry {
            result.push(def);
        }
        result.extend(curated);

        // Fallback: If everything was filtered out, return raw list
        if result.is_empty() {
            if let Ok(all_devs) = self.host.input_devices() {
                for (idx, dev) in all_devs.enumerate() {
                    let name = dev
                        .description()
                        .map(|d| d.name().to_string())
                        .unwrap_or_else(|_| format!("Device #{}", idx));
                    result.push((idx, name));
                }
            }
        }

        Ok(result)
    }

    /// Get default input device.
    pub fn default_input_device(&self) -> Option<Device> {
        self.host.default_input_device()
    }

    /// Get input device by index.
    pub fn get_input_device(&self, index: usize) -> Result<Device, RecorderError> {
        let mut devices = self.host.input_devices()?;
        devices
            .nth(index)
            .ok_or(RecorderError::DeviceNotFound(index))
    }

    /// Get detailed information for a specific input device.
    pub fn get_device_info(&self, index: usize, device: &Device) -> Result<DeviceInfo, RecorderError> {
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| format!("Device #{}", index));
        let default_config = device.default_input_config().ok();

        let (default_sample_rate, default_channels) = match &default_config {
            Some(c) => (c.sample_rate(), c.channels()),
            None => (48000, 1),
        };

        let mut rates = Vec::new();
        let mut channels = Vec::new();

        if let Ok(configs) = device.supported_input_configs() {
            for range in configs {
                let ch = range.channels();
                if !channels.contains(&ch) {
                    channels.push(ch);
                }
                let min_rate = range.min_sample_rate();
                let max_rate = range.max_sample_rate();
                for &candidate in &[16000, 24000, 32000, 44100, 48000, 96000] {
                    if candidate >= min_rate && candidate <= max_rate && !rates.contains(&candidate) {
                        rates.push(candidate);
                    }
                }
            }
        }
        rates.sort_unstable();
        channels.sort_unstable();

        Ok(DeviceInfo {
            id: index,
            name: Self::format_friendly_name(&name),
            default_sample_rate,
            default_channels,
            supported_sample_rates: rates,
            supported_channels: channels,
        })
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curated_devices() {
        let dm = DeviceManager::new();
        let devs = dm.list_input_devices().unwrap();
        println!("Curated devices list:");
        for (idx, name) in &devs {
            println!("  [{}] {}", idx, name);
        }
        assert!(!devs.is_empty());
        assert!(devs[0].1.starts_with("Default"));
    }
}
