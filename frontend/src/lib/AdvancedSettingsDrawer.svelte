<script lang="ts">
  import type { AdvancedSettingsDto } from './types';
  import { X, Sliders, ChevronDown, ChevronRight } from '@lucide/svelte';

  export let settings: AdvancedSettingsDto;
  export let onClose: () => void;
  export let onSettingsChange: () => void;
  export let onOpenDiagnostics: () => void;
</script>

<div class="drawer-backdrop" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="drawer-content" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <!-- Header -->
    <div class="drawer-header">
      <div class="header-title">
        <Sliders size={20} color="var(--accent-lime)" />
        <h2>Advanced Settings</h2>
      </div>
      <button class="close-btn" onclick={onClose} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <!-- Body Controls -->
    <div class="drawer-body">
      <div class="settings-items-list">
        <!-- Control 1: Conservative Vocal Bias -->
        <div class="setting-item">
          <div class="setting-labels">
            <span class="setting-name">Conservative Vocal Bias</span>
            <span class="setting-desc">Keep voice natural, avoid over-cleaning</span>
          </div>
          <div
            class="toggle-switch"
            class:active={settings.conservative_bias}
            onclick={() => {
              settings.conservative_bias = !settings.conservative_bias;
              onSettingsChange();
            }}
            role="switch"
            aria-checked={settings.conservative_bias}
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (settings.conservative_bias = !settings.conservative_bias, onSettingsChange())}
          >
            <div class="toggle-knob"></div>
          </div>
        </div>

        <!-- Control 2: De-clicker -->
        <div class="setting-item">
          <div class="setting-labels">
            <span class="setting-name">De-clicker</span>
            <span class="setting-desc">Remove small clicks and mouth noises</span>
          </div>
          <div
            class="toggle-switch"
            class:active={settings.declicker}
            onclick={() => {
              settings.declicker = !settings.declicker;
              onSettingsChange();
            }}
            role="switch"
            aria-checked={settings.declicker}
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (settings.declicker = !settings.declicker, onSettingsChange())}
          >
            <div class="toggle-knob"></div>
          </div>
        </div>

        <!-- Control 3: Breath / Pop Guard -->
        <div class="setting-item">
          <div class="setting-labels">
            <span class="setting-name">Breath / Pop Guard</span>
            <span class="setting-desc">Reduce breath, plosives and wind noise</span>
          </div>
          <div
            class="toggle-switch"
            class:active={settings.plosive_guard}
            onclick={() => {
              settings.plosive_guard = !settings.plosive_guard;
              onSettingsChange();
            }}
            role="switch"
            aria-checked={settings.plosive_guard}
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (settings.plosive_guard = !settings.plosive_guard, onSettingsChange())}
          >
            <div class="toggle-knob"></div>
          </div>
        </div>

        <!-- Control 4: Harmonic Preservation Slider -->
        <div class="setting-item-column">
          <div class="slider-header">
            <span class="setting-name">Harmonic Preservation</span>
            <span class="slider-value tabular-nums">{Math.round(settings.harmonic_preservation * 100)}%</span>
          </div>
          <span class="setting-desc">Keep natural tone and richness</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            class="lime-slider"
            style="background: linear-gradient(to right, var(--accent-lime) 0%, var(--accent-lime) {Math.round(settings.harmonic_preservation * 100)}%, #25252C {Math.round(settings.harmonic_preservation * 100)}%, #25252C 100%);"
            bind:value={settings.harmonic_preservation}
            onchange={onSettingsChange}
          />
        </div>

        <!-- Control 5: AI Engine Model Dropdown -->
        <div class="setting-item-column">
          <span class="setting-name">AI Engine Model</span>
          <div class="select-wrapper">
            <select
              class="select-input"
              bind:value={settings.model_id}
              onchange={onSettingsChange}
            >
              <option value="dpdfnet2_48k">DPDFNet2 (High Quality)</option>
              <option value="deepfilter_net3">DeepFilterNet3</option>
            </select>
            <ChevronDown size={16} class="select-chevron" />
          </div>
        </div>

        <!-- Control 6: Audio Diagnostics trigger row -->
        <div
          class="diagnostics-trigger-row"
          onclick={() => {
            onClose();
            onOpenDiagnostics();
          }}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && (onClose(), onOpenDiagnostics())}
        >
          <div class="diag-left">
            <div class="diag-bars">
              <span class="bar bar-1"></span>
              <span class="bar bar-2"></span>
              <span class="bar bar-3"></span>
              <span class="bar bar-4"></span>
            </div>
            <div class="diag-texts">
              <span class="diag-title">Audio Diagnostics</span>
              <span class="diag-desc">View noise profile, levels and analysis</span>
            </div>
          </div>
          <ChevronRight size={18} color="rgba(255,255,255,0.4)" />
        </div>
      </div>
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
    width: min(440px, 100vw);
    height: 100%;
    background: #141418;
    border-left: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: -10px 0 40px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    padding: clamp(20px, 3vh, 28px) clamp(18px, 2.5vw, 24px);
    box-sizing: border-box;
    animation: slideIn 0.22s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    flex-shrink: 0;
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
    letter-spacing: -0.3px;
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
    transition: all 0.15s ease;
  }

  .close-btn:hover {
    color: #FFFFFF;
    background: rgba(255, 255, 255, 0.08);
  }

  .drawer-body {
    display: flex;
    flex-direction: column;
    gap: 20px;
    overflow-y: auto;
    flex: 1;
    padding-right: 4px;
  }

  .settings-items-list {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .setting-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }

  .setting-labels {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .setting-name {
    font-size: 14px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .setting-desc {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
    line-height: 1.35;
  }

  .setting-item-column {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .slider-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .slider-value {
    font-size: 14px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .select-wrapper {
    position: relative;
    width: 100%;
    margin-top: 2px;
  }

  .select-input {
    width: 100%;
    background: #0B0B0E;
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #FFFFFF;
    padding: 10px 14px;
    border-radius: 12px;
    font-size: 13.5px;
    appearance: none;
    outline: none;
    cursor: pointer;
    font-weight: 500;
    transition: border-color 0.15s ease;
  }

  .select-input:hover {
    border-color: rgba(255, 255, 255, 0.2);
  }

  :global(.select-chevron) {
    position: absolute;
    right: 14px;
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
    color: rgba(255, 255, 255, 0.45);
  }

  .diagnostics-trigger-row {
    background: #0B0B0E;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 14px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.15s ease;
    margin-top: 6px;
  }

  .diagnostics-trigger-row:hover {
    border-color: rgba(255, 255, 255, 0.2);
    background: #121217;
  }

  .diag-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .diag-bars {
    display: flex;
    align-items: flex-end;
    gap: 2.5px;
    height: 22px;
  }

  .diag-bars .bar {
    width: 3px;
    background-color: #FFFFFF;
    border-radius: 9999px;
  }

  .diag-bars .bar-1 { height: 8px; }
  .diag-bars .bar-2 { height: 16px; }
  .diag-bars .bar-3 { height: 22px; }
  .diag-bars .bar-4 { height: 12px; }

  .diag-texts {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .diag-title {
    font-size: 13.5px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .diag-desc {
    font-size: 11.5px;
    color: rgba(255, 255, 255, 0.45);
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
