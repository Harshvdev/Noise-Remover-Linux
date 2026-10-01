<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { DeviceDto } from './types';
  import { X, Mic, Upload, Check } from '@lucide/svelte';

  export let onClose: () => void;
  export let onTrackImported: (() => void) | undefined = undefined;

  let devices: DeviceDto[] = [];
  let selectedIdx = 0;
  let isImporting = false;

  onMount(async () => {
    try {
      devices = await api.getDevices();
      const def = devices.find((d) => d.is_default);
      if (def) selectedIdx = def.index;
    } catch (e) {
      console.error(e);
    }
  });

  async function handleSelect(idx: number) {
    if (selectedIdx === idx) return;
    selectedIdx = idx;
    try {
      await api.selectDevice(idx);
    } catch (e) {
      console.error(e);
    }
  }

  async function handleFileSelect(e: Event) {
    const input = e.target as HTMLInputElement;
    if (input.files && input.files[0]) {
      const file = input.files[0];
      // In browser fallback or web context
      const fakePath = (file as any).path || file.name;
      try {
        isImporting = true;
        await api.importAudioFile(fakePath);
        if (onTrackImported) onTrackImported();
        onClose();
      } catch (err) {
        console.error(err);
      } finally {
        isImporting = false;
      }
    }
  }
</script>

<div class="drawer-backdrop" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="drawer-content" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="drawer-header">
      <div class="header-title">
        <Mic size={20} color="var(--accent-lime)" />
        <h2>Audio Settings</h2>
      </div>
      <button class="close-btn" onclick={onClose} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="drawer-body">
      <!-- Input Devices Section -->
      <section class="settings-section">
        <div class="section-label">Microphone Input</div>
        <div class="devices-list">
          {#each devices as device}
            <div
              class="device-option"
              class:selected={selectedIdx === device.index}
              onclick={() => handleSelect(device.index)}
              role="button"
              tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && handleSelect(device.index)}
            >
              <div class="device-info">
                <span class="device-name">{device.name}</span>
                {#if device.is_default}
                  <span class="default-badge">System Default</span>
                {/if}
              </div>
              {#if selectedIdx === device.index}
                <Check size={18} color="var(--accent-lime)" />
              {/if}
            </div>
          {/each}
        </div>
      </section>

      <!-- Audio Engine Specs -->
      <section class="settings-section">
        <div class="section-label">Processing Configuration</div>
        <div class="specs-grid">
          <div class="spec-card">
            <span class="spec-title">Sample Rate</span>
            <span class="spec-value">48,000 Hz</span>
          </div>
          <div class="spec-card">
            <span class="spec-title">Internal Precision</span>
            <span class="spec-value">Float32 (Full Band)</span>
          </div>
          <div class="spec-card">
            <span class="spec-title">Subsystem</span>
            <span class="spec-value">PipeWire / CPAL</span>
          </div>
          <div class="spec-card">
            <span class="spec-title">Denoise Model</span>
            <span class="spec-value">DPDFNet2 + DFNet3</span>
          </div>
        </div>
      </section>

      <!-- Import Audio File -->
      <section class="settings-section">
        <div class="section-label">Import Audio File</div>
        <label class="import-dropzone">
          <Upload size={24} color="var(--accent-lime)" />
          <span class="dropzone-text">Click or drag & drop audio file to clean</span>
          <span class="dropzone-sub">WAV, FLAC, MP3, OGG supported</span>
          <input
            type="file"
            accept="audio/*"
            class="hidden-file-input"
            onchange={handleFileSelect}
            disabled={isImporting}
          />
        </label>
      </section>
    </div>
  </div>
</div>

<style>
  .drawer-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(8px);
    z-index: 150;
    display: flex;
    justify-content: flex-end;
    animation: fadeIn 0.18s ease-out;
  }

  .drawer-content {
    width: 440px;
    height: 100%;
    background: #16161A;
    border-left: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: -10px 0 40px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    padding: 28px 24px;
    animation: slideIn 0.22s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .header-title h2 {
    font-family: var(--font-brand);
    font-size: 20px;
    font-weight: 700;
    color: #FFFFFF;
  }

  .close-btn {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn:hover {
    color: #FFFFFF;
    background: rgba(255, 255, 255, 0.08);
  }

  .drawer-body {
    display: flex;
    flex-direction: column;
    gap: 24px;
    overflow-y: auto;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .devices-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .device-option {
    background: #0B0B0D;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 12px 14px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .device-option:hover {
    border-color: rgba(255, 255, 255, 0.2);
  }

  .device-option.selected {
    border-color: var(--accent-lime);
    background: rgba(198, 255, 61, 0.04);
  }

  .device-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .device-name {
    font-size: 13.5px;
    font-weight: 500;
    color: #FFFFFF;
  }

  .default-badge {
    font-size: 11px;
    color: var(--accent-lime);
  }

  .specs-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .spec-card {
    background: #0B0B0D;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .spec-title {
    font-size: 11px;
    color: var(--text-muted);
  }

  .spec-value {
    font-size: 12.5px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .import-dropzone {
    border: 2px dashed rgba(255, 255, 255, 0.15);
    border-radius: 14px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    transition: all 0.18s ease;
  }

  .import-dropzone:hover {
    border-color: var(--accent-lime);
    background: rgba(198, 255, 61, 0.03);
  }

  .dropzone-text {
    font-size: 13px;
    font-weight: 500;
    color: #FFFFFF;
  }

  .dropzone-sub {
    font-size: 11px;
    color: var(--text-muted);
  }

  .hidden-file-input {
    display: none;
  }

  @keyframes slideIn {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
