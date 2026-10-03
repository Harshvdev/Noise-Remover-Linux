<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { DeviceDto } from './types';
  import { currentTheme, setTheme } from './theme';
  import { X, Mic, Upload, Check, Moon, Sun, Settings } from '@lucide/svelte';

  export let onClose: () => void;
  export let onTrackImported: (() => void) | undefined = undefined;

  let devices: DeviceDto[] = [
    { index: 0, name: 'Default Audio Input', is_default: true }
  ];
  let selectedIdx = 0;
  let isImporting = false;
  let isMobile = false;

  onMount(async () => {
    if (typeof navigator !== 'undefined') {
      isMobile = /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent);
    }
    try {
      const list = await api.getDevices();
      if (list && list.length > 0) {
        devices = list;
        const def = devices.find((d) => d.is_default);
        if (def) selectedIdx = def.index;
      }
    } catch (e) {
      console.error('Failed to load audio devices:', e);
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

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') onClose(); }} />

<div class="drawer-backdrop" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="drawer-content" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="drawer-header">
      <div class="header-title">
        <Settings size={20} color="var(--accent-lime)" />
        <h2>Settings</h2>
      </div>
      <button class="close-btn" onclick={onClose} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="drawer-body">
      <!-- Theme & Appearance Section -->
      <section class="settings-section">
        <div class="section-label">Appearance & Theme</div>
        <div class="theme-grid">
          <div
            class="theme-card"
            class:selected={$currentTheme === 'dark'}
            onclick={() => setTheme('dark')}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && setTheme('dark')}
          >
            <div class="theme-card-left">
              <div class="theme-icon dark-theme-icon">
                <Moon size={16} color="#C6FF3D" />
              </div>
              <div class="theme-meta">
                <div class="theme-name-row">
                  <span class="theme-title">Dark Theme</span>
                  <span class="theme-accent-pill pill-lime">Lime</span>
                </div>
                <span class="theme-desc">Obsidian dark with neon lime</span>
              </div>
            </div>
            {#if $currentTheme === 'dark'}
              <Check size={18} color="var(--accent-lime)" strokeWidth={2.4} />
            {/if}
          </div>

          <div
            class="theme-card"
            class:selected={$currentTheme === 'light'}
            onclick={() => setTheme('light')}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && setTheme('light')}
          >
            <div class="theme-card-left">
              <div class="theme-icon light-theme-icon">
                <Sun size={16} color="#FF6600" />
              </div>
              <div class="theme-meta">
                <div class="theme-name-row">
                  <span class="theme-title">Light Theme</span>
                  <span class="theme-accent-pill pill-orange">Orange</span>
                </div>
                <span class="theme-desc">Clean light studio with warm orange</span>
              </div>
            </div>
            {#if $currentTheme === 'light'}
              <Check size={18} color="var(--accent-lime)" strokeWidth={2.4} />
            {/if}
          </div>
        </div>
      </section>

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
            <span class="spec-value">{isMobile ? 'System Audio (AAudio / Oboe)' : 'PipeWire / CPAL'}</span>
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
          <span class="dropzone-text">{isMobile ? 'Tap to choose audio file to clean' : 'Click or drag & drop audio file to clean'}</span>
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
    background: var(--modal-backdrop);
    backdrop-filter: blur(8px);
    z-index: 150;
    display: flex;
    justify-content: flex-end;
    animation: fadeIn 0.18s ease-out;
  }

  .drawer-content {
    width: min(440px, 100vw);
    height: 100%;
    background: var(--bg-modal);
    border-left: 1px solid var(--border-subtle);
    box-shadow: var(--shadow-floating);
    display: flex;
    flex-direction: column;
    padding-top: max(clamp(16px, 3vh, 28px), calc(var(--safe-top, 0px) + 12px));
    padding-bottom: max(clamp(16px, 3vh, 28px), calc(var(--safe-bottom, 0px) + 16px));
    padding-left: max(clamp(16px, 3vw, 24px), calc(var(--safe-left, 0px) + 16px));
    padding-right: max(clamp(16px, 3vw, 24px), calc(var(--safe-right, 0px) + 16px));
    box-sizing: border-box;
    animation: slideIn 0.22s cubic-bezier(0.16, 1, 0.3, 1);
    color: var(--text-main);
  }

  .drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--border-subtle);
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
    color: var(--text-main);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .close-btn:hover {
    color: var(--text-main);
    background: var(--bg-card-hover);
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
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }

  /* Theme selection styling */
  .theme-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .theme-card {
    background: var(--bg-surface-sunken);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 12px 14px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.16s ease;
  }

  .theme-card:hover {
    border-color: var(--border-medium);
    background: var(--bg-card-hover);
  }

  .theme-card.selected {
    border-color: var(--accent-lime);
    background: var(--accent-lime-dim);
  }

  .theme-card-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .theme-icon {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dark-theme-icon {
    background: #0B0B0D;
    border: 1px solid rgba(255, 255, 255, 0.1);
  }

  .light-theme-icon {
    background: #FFFFFF;
    border: 1px solid rgba(0, 0, 0, 0.1);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
  }

  .theme-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .theme-name-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .theme-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-main);
  }

  .theme-accent-pill {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 9999px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .pill-lime {
    background: rgba(198, 255, 61, 0.2);
    color: #9ECE10;
  }

  :global([data-theme="dark"]) .pill-lime {
    color: #C6FF3D;
  }

  .pill-orange {
    background: rgba(255, 102, 0, 0.18);
    color: #FF6600;
  }

  .theme-desc {
    font-size: 12px;
    color: var(--text-muted);
  }

  .devices-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .device-option {
    background: var(--bg-surface-sunken);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 12px 14px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .device-option:hover {
    border-color: var(--border-medium);
    background: var(--bg-card-hover);
  }

  .device-option.selected {
    border-color: var(--accent-lime);
    background: var(--accent-lime-dim);
  }

  .device-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .device-name {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text-main);
  }

  .default-badge {
    font-size: 11px;
    color: var(--accent-lime);
    font-weight: 600;
  }

  .specs-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .spec-card {
    background: var(--bg-surface-sunken);
    border: 1px solid var(--border-subtle);
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
    color: var(--text-main);
  }

  .import-dropzone {
    border: 2px dashed var(--border-medium);
    border-radius: 14px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    background: var(--bg-surface-sunken);
    transition: all 0.18s ease;
  }

  .import-dropzone:hover {
    border-color: var(--accent-lime);
    background: var(--accent-lime-dim);
  }

  .dropzone-text {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-main);
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
