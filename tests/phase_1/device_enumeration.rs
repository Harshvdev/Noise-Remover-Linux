//! Tests covering device enumeration, PipeWire source parsing, and hardware port filtering.

use recorder::device::DeviceManager;

#[test]
fn test_device_manager_enumeration() {
    let dm = DeviceManager::new();
    let devices = dm.list_input_devices().expect("Must enumerate audio inputs");

    assert!(!devices.is_empty(), "Must detect at least one input device");

    for (idx, name) in &devices {
        println!("Detected Input [{}] {}", idx, name);
        assert!(!name.trim().is_empty(), "Device name cannot be empty");

        // Must not expose internal ALSA routing/dummy plugins to user
        let lower = name.to_lowercase();
        assert!(!lower.contains("dsnoop"), "Internal dsnoop plugin must be filtered");
        assert!(!lower.contains("dmix"), "Internal dmix plugin must be filtered");
        assert!(!lower.contains("rate converter"), "Rate converter plugin must be filtered");
        assert!(!lower.contains("surround"), "Surround plugin must be filtered");
    }
}

#[test]
fn test_pipewire_sources_detection() {
    let sources = DeviceManager::get_pipewire_sources();
    // On Linux systems with PipeWire, verify source structure
    for s in &sources {
        assert!(!s.name.is_empty(), "PipeWire source name must not be empty");
        assert!(!s.description.is_empty(), "Description must not be empty");
        println!("PipeWire Source: {} (Port: {:?}, Default: {})", s.description, s.port, s.is_default);
    }
}

#[test]
fn test_default_input_device_presence() {
    let dm = DeviceManager::new();
    let def = dm.default_input_device();
    // On Linux running sound server, default device must exist
    assert!(def.is_some(), "Default input device must be detectable");
}
