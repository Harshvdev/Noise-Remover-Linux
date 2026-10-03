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
    pub bluetooth_card: Option<String>,
}

#[cfg(target_os = "android")]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AndroidDeviceInfo {
    pub id: i32,
    pub name: String,
    pub tier: i32,
    pub is_default: bool,
    pub is_selected: bool,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum DeviceTier {
    WiredExternal = 0, // Highest priority: USB mic, external 3.5mm mic, external card
    Bluetooth = 1,     // Middle priority: Bluetooth headset/earbuds/mic
    Internal = 2,      // Lowest priority: Built-in laptop mic
}

/// Classify input device into priority tiers: Wired External > Bluetooth > Internal
pub fn get_device_tier(name: &str, port: Option<&str>, desc: &str) -> DeviceTier {
    let name_low = name.to_lowercase();
    let desc_low = desc.to_lowercase();
    let port_low = port.map(|p| p.to_lowercase()).unwrap_or_default();

    // 1. Bluetooth devices
    if name_low.contains("bluez")
        || desc_low.contains("bluetooth")
        || port_low.contains("headset-hf")
        || name_low.contains("rfcomm")
    {
        return DeviceTier::Bluetooth;
    }

    // 2. Wired External devices (USB microphones, 3.5mm line/headset jacks)
    if name_low.contains("usb")
        || desc_low.contains("usb")
        || port_low == "analog-input-mic"
        || port_low == "analog-input-headset-mic"
        || port_low == "analog-input-linein"
        || desc_low.contains("wired")
        || desc_low.contains("3.5mm")
        || desc_low.contains("external")
    {
        return DeviceTier::WiredExternal;
    }

    // 3. Internal devices (laptop built-in mics, dmic, ALC analog internal port)
    if port_low == "analog-input-internal-mic"
        || desc_low.contains("internal")
        || desc_low.contains("built-in")
        || desc_low.contains("dmic")
        || desc_low.contains("laptop")
    {
        return DeviceTier::Internal;
    }

    if name_low.contains("pci") && name_low.contains("analog") {
        return DeviceTier::Internal;
    }

    DeviceTier::WiredExternal
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

    /// Discover connected Bluetooth audio headsets from Linux sound subsystem (PipeWire / PulseAudio cards).
    fn detect_bluetooth_headsets() -> Vec<PipeWireSourceInfo> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("pactl")
                .args(["list", "cards"])
                .output()
                .ok();
            if let Some(out) = output {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let mut bt_list = Vec::new();
                    let mut cur_card_name = String::new();
                    let mut cur_card_desc = String::new();
                    let mut cur_is_bluez = false;
                    let mut cur_has_headset_profile = false;
                    let mut in_profiles = false;

                    let flush_card = |name: &mut String,
                                          desc: &mut String,
                                          is_bluez: bool,
                                          has_headset: bool,
                                          list: &mut Vec<PipeWireSourceInfo>| {
                        if is_bluez && has_headset && !name.is_empty() {
                            let friendly = if !desc.is_empty() {
                                format!("{} (Bluetooth Headset Mic)", desc)
                            } else {
                                "Bluetooth Headset Microphone".to_string()
                            };
                            list.push(PipeWireSourceInfo {
                                name: name.clone(),
                                port: None,
                                description: friendly,
                                is_default: false,
                                is_muted: false,
                                volume_percent: 100,
                                bluetooth_card: Some(name.clone()),
                            });
                        }
                        name.clear();
                        desc.clear();
                    };

                    for line in text.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("Card #") {
                            flush_card(
                                &mut cur_card_name,
                                &mut cur_card_desc,
                                cur_is_bluez,
                                cur_has_headset_profile,
                                &mut bt_list,
                            );
                            cur_is_bluez = false;
                            cur_has_headset_profile = false;
                            in_profiles = false;
                        } else if let Some(n) = trimmed.strip_prefix("Name: ") {
                            cur_card_name = n.trim().to_string();
                            if cur_card_name.contains("bluez") {
                                cur_is_bluez = true;
                            }
                        } else if let Some(d) = trimmed.strip_prefix("device.description = ") {
                            cur_card_desc = d.trim().trim_matches('"').to_string();
                        } else if let Some(a) = trimmed.strip_prefix("device.alias = ") {
                            if cur_card_desc.is_empty() {
                                cur_card_desc = a.trim().trim_matches('"').to_string();
                            }
                        } else if trimmed.starts_with("Profiles:") {
                            in_profiles = true;
                        } else if in_profiles {
                            if trimmed.starts_with("Active Profile:") {
                                in_profiles = false;
                            } else if trimmed.contains("headset-head-unit")
                                && !trimmed.contains("available: no")
                            {
                                cur_has_headset_profile = true;
                            }
                        }
                    }
                    flush_card(
                        &mut cur_card_name,
                        &mut cur_card_desc,
                        cur_is_bluez,
                        cur_has_headset_profile,
                        &mut bt_list,
                    );
                    return bt_list;
                }
            }
        }
        Vec::new()
    }

    /// Discover an active bluez input source from PipeWire / PulseAudio.
    pub fn find_active_bluez_source() -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("pactl")
                .args(["list", "sources", "short"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1].starts_with("bluez_input") {
                    return Some(parts[1].to_string());
                }
            }
        }
        None
    }

    /// Restore connected Bluetooth audio headsets to high-fidelity stereo playback (A2DP).
    pub fn restore_bluetooth_cards_to_a2dp() {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("pactl")
                .args(["list", "cards"])
                .output()
                .ok();
            if let Some(out) = output {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    for line in text.lines() {
                        let trimmed = line.trim();
                        if let Some(card_name) = trimmed.strip_prefix("Name: bluez_card.") {
                            let full_card = format!("bluez_card.{}", card_name.trim());
                            let _ = Command::new("pactl")
                                .args(["set-card-profile", &full_card, "a2dp-sink-sbc_xq"])
                                .output();
                            let _ = Command::new("pactl")
                                .args(["set-card-profile", &full_card, "a2dp-sink"])
                                .output();
                        }
                    }
                }
            }
        }
    }

    /// Forcibly move any active app capture streams to the specified target source if needed.
    pub fn move_app_stream_to_source(target_source: &str) {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("pactl")
                .args(["list", "source-outputs"])
                .output()
                .ok();
            if let Some(out) = output {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let mut cur_id = None;
                    for line in text.lines() {
                        let trimmed = line.trim();
                        if let Some(id_str) = trimmed.strip_prefix("Source Output #") {
                            cur_id = Some(id_str.trim().to_string());
                        }
                        if (trimmed.contains("PipeWire ALSA [app]")
                            || trimmed.contains("alsa_capture.app"))
                            && cur_id.is_some()
                        {
                            if let Some(ref id) = cur_id {
                                let _ = Command::new("pactl")
                                    .args(["move-source-output", id, target_source])
                                    .output();
                            }
                        }
                    }
                }
            }
        }
    }

    /// Query Android sound manager for active input sources (Wired, Bluetooth, Internal) via JNI.
    #[cfg(target_os = "android")]
    pub fn get_android_devices() -> Vec<AndroidDeviceInfo> {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            log::warn!("Android JavaVM pointer is null in DeviceManager");
            return Vec::new();
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let json_result: Result<String, jni::errors::Error> = vm.attach_current_thread(|env| {
            let res = env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/AudioDeviceHelper"),
                jni::jni_str!("getDevicesJson"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?;
            let obj = res.l()?;
            if obj.is_null() {
                return Ok(String::new());
            }
            let jstr = unsafe { jni::objects::JString::from_raw(env, obj.into_raw()) };
            let rust_str: String = env.get_string(&jstr)?.into();
            Ok(rust_str)
        });

        match json_result {
            Ok(json_str) => serde_json::from_str(&json_str).unwrap_or_default(),
            Err(e) => {
                log::warn!("Failed to call AudioDeviceHelper.getDevicesJson(): {:?}", e);
                Vec::new()
            }
        }
    }

    /// Select an audio input device on Android by its AudioDeviceInfo ID.
    #[cfg(target_os = "android")]
    pub fn select_android_device(id: i32) -> bool {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return false;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let res: Result<bool, jni::errors::Error> = vm.attach_current_thread(|env| {
            let val = jni::objects::JValue::from(id);
            let res = env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/AudioDeviceHelper"),
                jni::jni_str!("selectDevice"),
                jni::jni_sig!("(I)Z"),
                &[val],
            )?;
            Ok(res.z().unwrap_or(false))
        });
        res.unwrap_or(false)
    }

    /// Select the highest priority audio input device on Android (Wired > Bluetooth > Internal).
    #[cfg(target_os = "android")]
    pub fn select_android_default_device() -> bool {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return false;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let res: Result<bool, jni::errors::Error> = vm.attach_current_thread(|env| {
            let res = env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/AudioDeviceHelper"),
                jni::jni_str!("selectDefaultDevice"),
                jni::jni_sig!("()Z"),
                &[],
            )?;
            Ok(res.z().unwrap_or(false))
        });
        res.unwrap_or(false)
    }

    /// Notify Android AudioManager when recording or calibration starts.
    #[cfg(target_os = "android")]
    pub fn notify_android_recording_started() {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let _: Result<(), jni::errors::Error> = vm.attach_current_thread(|env| {
            env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/AudioDeviceHelper"),
                jni::jni_str!("onRecordingStarted"),
                jni::jni_sig!("()Z"),
                &[],
            )?;
            Ok(())
        });
    }

    /// Notify Android AudioManager when recording or calibration stops.
    #[cfg(target_os = "android")]
    pub fn notify_android_recording_stopped() {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let _: Result<(), jni::errors::Error> = vm.attach_current_thread(|env| {
            env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/AudioDeviceHelper"),
                jni::jni_str!("onRecordingStopped"),
                jni::jni_sig!("()Z"),
                &[],
            )?;
            Ok(())
        });
    }

    /// Share a recorded track on Android using native Share Sheet.
    #[cfg(target_os = "android")]
    pub fn share_android_file(file_path: &str, title: &str) -> bool {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return false;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let res: Result<(), jni::errors::Error> = vm.attach_current_thread(|env| {
            let j_path = env.new_string(file_path)?;
            let j_title = env.new_string(title)?;
            let val_path = jni::objects::JValue::from(&j_path);
            let val_title = jni::objects::JValue::from(&j_title);
            env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/MainActivity"),
                jni::jni_str!("shareTrack"),
                jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                &[val_path, val_title],
            )?;
            Ok(())
        });
        res.is_ok()
    }

    /// Export a recorded track to Android's public Music/VoiceCleaner directory.
    #[cfg(target_os = "android")]
    pub fn export_android_file(file_path: &str, title: &str) -> bool {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return false;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let res: Result<bool, jni::errors::Error> = vm.attach_current_thread(|env| {
            let j_path = env.new_string(file_path)?;
            let j_title = env.new_string(title)?;
            let val_path = jni::objects::JValue::from(&j_path);
            let val_title = jni::objects::JValue::from(&j_title);
            let res = env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/MainActivity"),
                jni::jni_str!("exportTrack"),
                jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Z"),
                &[val_path, val_title],
            )?;
            Ok(res.z().unwrap_or(false))
        });
        res.unwrap_or(false)
    }

    /// Move app to back / exit on Android
    #[cfg(target_os = "android")]
    pub fn exit_android_app() -> bool {
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return false;
        }
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let res: Result<(), jni::errors::Error> = vm.attach_current_thread(|env| {
            env.call_static_method(
                jni::jni_str!("com/voicecleaner/app/MainActivity"),
                jni::jni_str!("exitApp"),
                jni::jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        });
        res.is_ok()
    }

    #[cfg(not(target_os = "android"))]
    pub fn share_android_file(_file_path: &str, _title: &str) -> bool {
        false
    }

    #[cfg(not(target_os = "android"))]
    pub fn export_android_file(_file_path: &str, _title: &str) -> bool {
        false
    }

    #[cfg(not(target_os = "android"))]
    pub fn exit_android_app() -> bool {
        false
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
                                    "Bluetooth Headset Mic".to_string()
                                } else {
                                    format!("{} (Bluetooth Headset Mic)", desc)
                                };
                                let is_def = *name == default_src;
                                list.push(PipeWireSourceInfo {
                                    name: name.clone(),
                                    port: None,
                                    description: friendly,
                                    is_default: is_def,
                                    is_muted,
                                    volume_percent: volume_pct,
                                    bluetooth_card: None,
                                });
                            } else if !ports.is_empty() {
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
                                        bluetooth_card: None,
                                    });
                                }
                            } else {
                                let p = if !active_port.is_empty() && active_port != "analog-input-mic" {
                                    Some(active_port.clone())
                                } else {
                                    None
                                };
                                let friendly = if p.as_deref() == Some("analog-input-internal-mic") {
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
                                    bluetooth_card: None,
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
                            // Only include ports that are plugged in / available (filter out "not available")
                            if !trimmed.contains("not available") {
                                if let Some(port_name) = trimmed.split(':').next() {
                                    cur_ports.push(port_name.trim().to_string());
                                }
                            }
                        }
                    }
                    flush(&mut cur_name, &mut cur_desc, &mut cur_active_port, &mut cur_ports, cur_is_muted, cur_volume_pct, &mut is_monitor, &mut list);

                    // Discover connected Bluetooth audio headsets (e.g. in A2DP mode without capture source yet)
                    let bt_cards = Self::detect_bluetooth_headsets();
                    for bt in bt_cards {
                        let already_present = list.iter().any(|s| {
                            if let Some(ref card) = bt.bluetooth_card {
                                s.name.contains(card) || s.description.contains(&bt.description)
                            } else {
                                false
                            }
                        });
                        if !already_present {
                            list.push(bt);
                        }
                    }

                    // Sort input devices by priority: Wired External > Bluetooth > Internal
                    list.sort_by_key(|s| get_device_tier(&s.name, s.port.as_deref(), &s.description));

                    // Default to external microphone if available (Wired > Bluetooth > Internal)
                    if !list.is_empty() {
                        for s in list.iter_mut() {
                            s.is_default = false;
                        }
                        list[0].is_default = true;
                    }

                    if !list.is_empty() {
                        return list;
                    }
                }
            }
        }
        Vec::new()
    }

    /// Select an active PipeWire source by index, switching hardware port, unmuting, and ensuring adequate volume.
    /// Optimized for high responsiveness with minimal latency and no unnecessary thread sleeps.
    pub fn select_pipewire_source(&self, index: usize) -> Result<(), RecorderError> {
        let sources = Self::get_pipewire_sources();
        if let Some(src) = sources.get(index) {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;

                let is_bluetooth = src.name.contains("bluez")
                    || src.bluetooth_card.is_some()
                    || src.description.to_lowercase().contains("bluetooth");

                let mut active_source_name = src.name.clone();

                if is_bluetooth {
                    // Only renegotiate Bluetooth profile if this card does not already have an active input source
                    if !active_source_name.starts_with("bluez_input") {
                        if let Some(ref card_name) = src.bluetooth_card {
                            let _ = Command::new("pactl")
                                .args(["set-card-profile", card_name, "headset-head-unit-msbc"])
                                .output();
                            let _ = Command::new("pactl")
                                .args(["set-card-profile", card_name, "headset-head-unit"])
                                .output();

                            for _ in 0..5 {
                                if let Some(found_src) = Self::find_active_bluez_source() {
                                    active_source_name = found_src;
                                    break;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(20));
                            }
                        }
                    }
                } else {
                    // Switching to internal or wired hardware mic
                    if let Some(ref port) = src.port {
                        let _ = Command::new("pactl")
                            .args(["set-source-port", &src.name, port])
                            .output();
                    }
                    // Restore Bluetooth headphones to high-fidelity stereo A2DP playback when switching away
                    Self::restore_bluetooth_cards_to_a2dp();
                }

                // 1. Set default source in PipeWire / PulseAudio
                let _ = Command::new("pactl")
                    .args(["set-default-source", &active_source_name])
                    .output();

                // 2. Unmute source explicitly in PipeWire
                let _ = Command::new("pactl")
                    .args(["set-source-mute", &active_source_name, "0"])
                    .output();

                // 3. Ensure appropriate capture volume without overdriving
                if is_bluetooth {
                    if src.volume_percent == 0 {
                        let _ = Command::new("pactl")
                            .args(["set-source-volume", &active_source_name, "80%"])
                            .output();
                    }
                } else if src.name.contains("usb") {
                    if src.volume_percent == 0 {
                        let _ = Command::new("pactl")
                            .args(["set-source-volume", &active_source_name, "75%"])
                            .output();
                    }
                } else {
                    // For ALSA internal analog mic: normalize volume to 25% if zero or boosted
                    if src.volume_percent == 0 || src.volume_percent > 35 {
                        let _ = Command::new("pactl")
                            .args(["set-source-volume", &active_source_name, "25%"])
                            .output();
                    }
                    let _ = Command::new("amixer")
                        .args(["sset", "Capture", "cap"])
                        .output();
                }
            }
            Ok(())
        } else {
            Err(RecorderError::DeviceNotFound(index))
        }
    }

    /// Get current volume percent (0-100) for the specified source.
    pub fn get_pipewire_source_volume(&self, index: usize) -> Option<u32> {
        let sources = Self::get_pipewire_sources();
        let src = sources.get(index)?;
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("pactl")
                .args(["get-source-volume", &src.name])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Some(pct_idx) = text.find('%') {
                    let prefix = &text[..pct_idx];
                    if let Some(val_str) = prefix.split_whitespace().last() {
                        if let Ok(val) = val_str.parse::<u32>() {
                            return Some(val);
                        }
                    }
                }
            }
        }
        Some(src.volume_percent)
    }

    /// Set volume percent (0-100) for the specified source.
    pub fn set_pipewire_source_volume(&self, index: usize, volume_percent: u32) -> Result<(), RecorderError> {
        let sources = Self::get_pipewire_sources();
        if let Some(src) = sources.get(index) {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                let vol_str = format!("{}%", volume_percent.clamp(0, 100));
                let _ = Command::new("pactl")
                    .args(["set-source-volume", &src.name, &vol_str])
                    .output();
                // If setting analog mic to <= 35%, also reset hardware boost to 0dB
                if src.bluetooth_card.is_none() && volume_percent <= 35 {
                    let _ = Command::new("amixer")
                        .args(["sset", "Internal Mic Boost", "0"])
                        .output();
                }
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
        #[cfg(target_os = "android")]
        {
            let android_devs = Self::get_android_devices();
            if !android_devs.is_empty() {
                let mut list = Vec::new();
                for dev in android_devs {
                    list.push((dev.id as usize, dev.name));
                }
                return Ok(list);
            }
        }

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
        #[cfg(target_os = "android")]
        {
            let _ = Self::select_android_device(index as i32);
            return self.default_input_device().ok_or(RecorderError::DeviceNotFound(index));
        }

        #[cfg(not(target_os = "android"))]
        {
            let mut devices = self.host.input_devices()?;
            devices
                .nth(index)
                .ok_or(RecorderError::DeviceNotFound(index))
        }
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

/// Find optimal input configuration preferring canonical 1-channel mono at 48,000 Hz.
pub fn find_best_input_config(device: &Device) -> Result<cpal::SupportedStreamConfig, RecorderError> {
    if let Ok(configs) = device.supported_input_configs() {
        let configs_vec: Vec<_> = configs.collect();

        // 1. Mono (1 channel) at 48,000 Hz (F32 preferred, then I16, then I32)
        for fmt in [cpal::SampleFormat::F32, cpal::SampleFormat::I16, cpal::SampleFormat::I32] {
            if let Some(range) = configs_vec.iter().find(|c| {
                c.channels() == 1
                    && c.sample_format() == fmt
                    && c.min_sample_rate() <= 48000
                    && c.max_sample_rate() >= 48000
            }) {
                return Ok(range.with_sample_rate(48000));
            }
        }

        // 2. Mono (1 channel) with standard sample rate
        for fmt in [cpal::SampleFormat::F32, cpal::SampleFormat::I16, cpal::SampleFormat::I32] {
            if let Some(range) = configs_vec.iter().find(|c| c.channels() == 1 && c.sample_format() == fmt) {
                let target_rate = if range.min_sample_rate() <= 48000 && range.max_sample_rate() >= 48000 {
                    48000
                } else {
                    range.max_sample_rate().min(48000).max(range.min_sample_rate())
                };
                return Ok(range.with_sample_rate(target_rate));
            }
        }

        // 3. Stereo (2 channels) at 48,000 Hz
        for fmt in [cpal::SampleFormat::F32, cpal::SampleFormat::I16, cpal::SampleFormat::I32] {
            if let Some(range) = configs_vec.iter().find(|c| {
                c.channels() == 2
                    && c.sample_format() == fmt
                    && c.min_sample_rate() <= 48000
                    && c.max_sample_rate() >= 48000
            }) {
                return Ok(range.with_sample_rate(48000));
            }
        }
    }

    device.default_input_config().map_err(Into::into)
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
