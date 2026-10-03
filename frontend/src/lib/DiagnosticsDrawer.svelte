<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { DiagnosticsDto } from './types';
  import { X, Activity, BarChart2, Radio, Zap } from '@lucide/svelte';

  export let trackId: string;
  export let onClose: () => void;

  let diagnostics: DiagnosticsDto | null = null;
  let loading = true;

  onMount(async () => {
    try {
      diagnostics = await api.getDiagnostics(trackId);
    } catch (e) {
      console.error(e);
    } finally {
      loading = false;
    }
  });
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') onClose(); }} />

<div class="drawer-backdrop" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="drawer-content" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="drawer-header">
      <div class="header-title">
        <Activity size={20} color="var(--accent-lime)" />
        <h2>Audio Diagnostics</h2>
      </div>
      <button class="close-btn" onclick={onClose} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    {#if loading}
      <div class="loading-state">Analyzing acoustic spectrum...</div>
    {:else if diagnostics}
      <div class="drawer-body">
        <!-- Frequency Spectrum PSD Canvas -->
        <section class="diag-section">
          <div class="section-label">Noise Power Spectral Density (PSD)</div>
          <div class="spectrum-card">
            <div class="spectrum-bars">
              {#each diagnostics.spectrum_bins as bin}
                {@const heightPct = Math.max(8, ((bin + 90) / 90) * 100)}
                <div
                  class="spectrum-bar"
                  style="height: {heightPct}%;"
                  title="{bin.toFixed(1)} dBFS"
                ></div>
              {/each}
            </div>
            <div class="spectrum-labels">
              <span>20 Hz</span>
              <span>1 kHz</span>
              <span>4 kHz</span>
              <span>12 kHz</span>
              <span>24 kHz</span>
            </div>
          </div>
        </section>

        <!-- Metrics Grid -->
        <section class="diag-section">
          <div class="section-label">Acoustic Analysis Metrics</div>
          <div class="metrics-grid">
            <div class="metric-card">
              <div class="metric-icon"><Zap size={16} color="var(--accent-lime)" /></div>
              <span class="metric-label">Estimated SNR</span>
              <span class="metric-val">+{diagnostics.estimated_snr_db.toFixed(1)} dB</span>
            </div>

            <div class="metric-card">
              <div class="metric-icon"><BarChart2 size={16} color="var(--accent-lime)" /></div>
              <span class="metric-label">Noise Floor</span>
              <span class="metric-val">{diagnostics.peak_noise_level_dbfs.toFixed(1)} dBFS</span>
            </div>

            <div class="metric-card">
              <div class="metric-icon"><Radio size={16} color="var(--accent-lime)" /></div>
              <span class="metric-label">Stationarity</span>
              <span class="metric-val">{Math.round(diagnostics.stationarity_score * 100)}%</span>
            </div>

            <div class="metric-card">
              <div class="metric-icon"><Activity size={16} color="var(--accent-lime)" /></div>
              <span class="metric-label">Tonal Peaks</span>
              <span class="metric-val">{diagnostics.tonal_peaks_count} detected</span>
            </div>
          </div>
        </section>
      </div>
    {:else}
      <div class="empty-state">No diagnostic data available for this take.</div>
    {/if}
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

  .diag-section {
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

  .spectrum-card {
    background: var(--bg-surface-sunken);
    border: 1px solid var(--border-subtle);
    border-radius: 14px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .spectrum-bars {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 140px;
    width: 100%;
  }

  .spectrum-bar {
    flex: 1;
    background: var(--accent-lime);
    border-radius: 2px 2px 0 0;
    opacity: 0.85;
    transition: height 0.2s ease;
  }

  .spectrum-bar:hover {
    opacity: 1;
  }

  .spectrum-labels {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-dim);
  }

  .metrics-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .metric-card {
    background: var(--bg-surface-sunken);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .metric-icon {
    margin-bottom: 2px;
  }

  .metric-label {
    font-size: 12px;
    color: var(--text-muted);
  }

  .metric-val {
    font-size: 15px;
    font-weight: 700;
    color: var(--text-main);
  }

  .loading-state, .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: var(--text-muted);
    font-size: 14px;
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
