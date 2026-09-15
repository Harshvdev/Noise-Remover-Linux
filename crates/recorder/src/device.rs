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

#[derive(Debug, Clone)]
pub struct PipeWireSourceInfo {
    pub name: String,
    pub port: Option<String>,
    pub description: String,
    pub is_default: bool,
    pub is_muted: bool,
    pub volume_percent: u32,
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

    /// Query Linux sound server (PipeWire / PulseAudio) for active input sources and hardware ports.
    pub fn get_pipewire_sources() -> Vec<PipeWireSourceInfo> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("pactl")
                .args(["list", "sources"])
                .output()
                .ok();
            if let Some(out) = output {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let default_src = Command::new("pactl")
                        .args(["get-default-source"])
                        .output()
                        .ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_default();

                    let mut list = Vec::new();
                    let mut cur_name = String::new();
                    let mut cur_desc = String::new();
                    let mut cur_active_port = String::new();
                    let mut cur_ports = Vec::new();
                    let mut cur_is_muted = false;
                    let mut cur_volume_pct = 100u32;
                    let mut is_monitor = false;

                    let flush = |name: &mut String,
                                 desc: &mut String,
                                 active_port: &mut String,
                                 ports: &mut Vec<String>,
                                 is_muted: bool,
                                 volume_pct: u32,
                                 monitor: &mut bool,
                                 list: &mut Vec<PipeWireSourceInfo>| {
                        if !name.is_empty() && !*monitor {
                            if name.contains("bluez") {
                                let friendly = if desc.is_empty() {
                                    "Bluetooth Headset".to_string()
                                } else {
                                    format!("{} (Bluetooth)", desc)
                                };
                                let is_def = *name == default_src;
                                list.push(PipeWireSourceInfo {
                                    name: name.clone(),
                                    port: None,
                                    description: friendly,
                                    is_default: is_def,
                                    is_muted,
                                    volume_percent: volume_pct,
                                });
                            } else if ports.len() > 1 {
                                for p in ports.iter() {
                                    let friendly = if p == "analog-input-mic" {
                                        "Wired Earphones (3.5mm Headset Mic)".to_string()
                                    } else if p == "analog-input-internal-mic" {
                                        "Built-in Microphone (Laptop Internal)".to_string()
                                    } else {
                                        format!("{} ({})", desc, p)
                                    };
                                    let is_def = *name == default_src && *active_port == *p;
                                    list.push(PipeWireSourceInfo {
                                        name: name.clone(),
                                        port: Some(p.clone()),
                                        description: friendly,
                                        is_default: is_def,
                                        is_muted,
                                        volume_percent: volume_pct,
                                    });
                                }
                            } else {
                                let p = if !active_port.is_empty() {
                                    Some(active_port.clone())
                                } else {
                                    None
                                };
                                let friendly = if p.as_deref() == Some("analog-input-mic") {
                                    "Wired Earphones (3.5mm Headset Mic)".to_string()
                                } else if p.as_deref() == Some("analog-input-internal-mic") {
                                    "Built-in Microphone (Laptop Internal)".to_string()
                                } else if !desc.is_empty() {
                                    desc.clone()
                                } else {
                                    name.clone()
                                };
                                let is_def = *name == default_src;
                                list.push(PipeWireSourceInfo {
                                    name: name.clone(),
                                    port: p,
                                    description: friendly,
                                    is_default: is_def,
                                    is_muted,
                                    volume_percent: volume_pct,
                                });
                            }
                        }
                        name.clear();
                        desc.clear();
                        active_port.clear();
                        ports.clear();
                        *monitor = false;
                    };

                    for line in text.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("Source #") {
                            flush(&mut cur_name, &mut cur_desc, &mut cur_active_port, &mut cur_ports, cur_is_muted, cur_volume_pct, &mut is_monitor, &mut list);
                            cur_is_muted = false;
                            cur_volume_pct = 100;
                        } else if let Some(n) = trimmed.strip_prefix("Name: ") {
                            cur_name = n.trim().to_string();
                            if cur_name.contains(".monitor") {
                                is_monitor = true;
                            }
                        } else if let Some(d) = trimmed.strip_prefix("Description: ") {
                            cur_desc = d.trim().to_string();
                        } else if let Some(p) = trimmed.strip_prefix("Active Port: ") {
                            cur_active_port = p.trim().to_string();
                        } else if let Some(m) = trimmed.strip_prefix("Mute: ") {
                            cur_is_muted = m.trim().eq_ignore_ascii_case("yes");
                        } else if trimmed.starts_with("Volume:") {
                            if let Some(pct_idx) = trimmed.find('%') {
                                let prefix = &trimmed[..pct_idx];
                                if let Some(val_str) = prefix.split_whitespace().last() {
                                    if let Ok(val) = val_str.parse::<u32>() {
                                        cur_volume_pct = val;
                                    }
                                }
                            }
                        } else if trimmed.starts_with("analog-input-") {
                            if let Some(port_name) = trimmed.split(':').next() {
                                cur_ports.push(port_name.trim().to_string());
                            }
                        }
                    }
                    flush(&mut cur_name, &mut cur_desc, &mut cur_active_port, &mut cur_ports, cur_is_muted, cur_volume_pct, &mut is_monitor, &mut list);

                    if !list.is_empty() {
                        return list;
                    }
                }
            }
        }
        Vec::new()
    }

    /// Select an active PipeWire source by index, switching hardware port, unmuting, and ensuring adequate volume.
    pub fn select_pipewire_source(&self, index: usize) -> Result<(), RecorderError> {
        let sources = Self::get_pipewire_sources();
        if let Some(src) = sources.get(index) {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                // 1. Set default source in PipeWire / PulseAudio
                let _ = Command::new("pactl")
                    .args(["set-default-source", &src.name])
                    .output();

                // 2. Switch hardware port if multi-port source (e.g. wired earphones vs internal mic)
                if let Some(ref port) = src.port {
                    let _ = Command::new("pactl")
                        .args(["set-source-port", &src.name, port])
                        .output();
                }

                // 3. Unmute source explicitly (WirePlumber / ALSA often default ports to muted)
                let _ = Command::new("pactl")
                    .args(["set-source-mute", &src.name, "0"])
                    .output();

                // 4. Ensure adequate capture volume if whisper-quiet (< 40%)
                if src.volume_percent < 40 {
                    let _ = Command::new("pactl")
                        .args(["set-source-volume", &src.name, "80%"])
                        .output();
                }

                // 5. Unmute hardware ALSA capture switch if present
                let _ = Command::new("amixer")
                    .args(["-D", "pulse", "sset", "Capture", "cap"])
                    .output();
                let _ = Command::new("amixer")
                    .args(["sset", "Capture", "cap"])
                    .output();

                // Brief settling delay for PipeWire graph routing
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            Ok(())
        } else {
            Err(RecorderError::DeviceNotFound(index))
        }
    }

    /// Check if a PipeWire source is muted in Linux system settings.
    pub fn is_pipewire_source_muted(&self, index: usize) -> bool {
        let sources = Self::get_pipewire_sources();
        if let Some(src) = sources.get(index) {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                if let Ok(output) = Command::new("pactl")
                    .args(["get-source-mute", &src.name])
                    .output()
                {
                    let text = String::from_utf8_lossy(&output.stdout);
                    return text.contains("Mute: yes") || text.trim() == "yes";
                }
            }
            src.is_muted
        } else {
            false
        }
    }

    /// Unmute the specified PipeWire source in Linux system settings.
    pub fn unmute_pipewire_source(&self, index: usize) -> Result<(), RecorderError> {
        let sources = Self::get_pipewire_sources();
        if let Some(src) = sources.get(index) {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                let _ = Command::new("pactl")
                    .args(["set-source-mute", &src.name, "0"])
                    .output();
                let _ = Command::new("amixer")
                    .args(["-D", "pulse", "sset", "Capture", "cap"])
                    .output();
                let _ = Command::new("amixer")
                    .args(["sset", "Capture", "cap"])
                    .output();
            }
            Ok(())
        } else {
            Err(RecorderError::DeviceNotFound(index))
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
            || lower.starts_with("pipewire sound server")
            || lower.starts_with("pulseaudio sound server")
    }

    /// Format a friendly human-readable name for audio devices.
    fn format_friendly_name(name: &str) -> String {
        let lower = name.to_lowercase();
        if lower.contains("default") || lower == "pipewire" || lower == "pulse" {
            return "Default Audio Input".to_string();
        }
        if lower.contains("alc257") || lower.contains("generic") {
            return "Built-in Microphone (ALC257 Analog)".to_string();
        }
        name.to_string()
    }

    /// List cleaned and curated available input devices.
    /// Prefers PipeWire/PulseAudio input sources if available.
    pub fn list_input_devices(&self) -> Result<Vec<(usize, String)>, RecorderError> {
        let pw_sources = Self::get_pipewire_sources();
        if !pw_sources.is_empty() {
            let mut list = Vec::new();
            for (idx, src) in pw_sources.iter().enumerate() {
                list.push((idx, src.description.clone()));
            }
            return Ok(list);
        }

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

            if friendly_name.starts_with("Default") {
                if default_entry.is_none() {
                    default_entry = Some((idx, friendly_name));
                }
                continue;
            }

            if !curated.iter().any(|(_, n): &(usize, String)| *n == friendly_name) {
                curated.push((idx, friendly_name));
            }
        }

        let mut result = Vec::new();
        if let Some(def) = default_entry {
            result.push(def);
        }
        result.extend(curated);

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
    }
}
