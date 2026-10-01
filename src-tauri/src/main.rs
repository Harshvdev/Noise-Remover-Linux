// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Critical fix for Linux WebKitGTK (Wayland / Mesa / AMD / Intel):
    // Disables the DMA-BUF renderer which fails with blank/white screen on Mesa drivers.
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    voice_cleaner_lib::run();
}
