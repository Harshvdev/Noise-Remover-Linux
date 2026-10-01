<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from './api';
  import type { AdvancedSettingsDto, TrackMetadata } from './types';
  import SplitWaveformScrubber from './SplitWaveformScrubber.svelte';
  import {
    ChevronLeft,
    MoreHorizontal,
    Activity,
    Star,
    Scissors,
    Upload,
    ChevronRight,
    ChevronDown,
  } from '@lucide/svelte';

  export let trackId: string;
  export let onNavigateBack: () => void;
  export let onOpenDiagnostics: (id: string) => void;

  let track: TrackMetadata | null = null;
  let isPlaying = false;
  let currentPosSecs = 34.0;
  let isCleanAudio = true;
  let isReprocessing = false;

  // Options Menu Popover
  let showMoreMenu = false;
  let isRenaming = false;
  let renameValue = '';

  // Advanced Settings State (defaults matching panel-3.png)
  let settings: AdvancedSettingsDto = {
    conservative_bias: true,
    declicker: true,
    plosive_guard: true,
    harmonic_preservation: 0.7,
    model_id: 'dpdfnet2_48k',
  };

  let statusPollInterval: ReturnType<typeof setInterval>;

  async function loadTrackData() {
    try {
      track = await api.getTrack(trackId);
      if (track) {
        renameValue = track.title;
      }
    } catch (e) {
      console.error('Failed to load track detail:', e);
    }
  }

  onMount(() => {
    loadTrackData();

    // Auto-start playback preview if desired or poll status
    statusPollInterval = setInterval(async () => {
      try {
        const status = await api.getPlaybackStatus();
        if (status.track_id === trackId) {
          isPlaying = status.is_playing;
          currentPosSecs = status.position_seconds;
          isCleanAudio = status.is_clean;
        }
      } catch (e) {
        if (isPlaying && track) {
          currentPosSecs = (currentPosSecs + 0.2) % track.duration_secs;
        }
      }
    }, 200);
  });

  onDestroy(() => {
    if (statusPollInterval) clearInterval(statusPollInterval);
  });

  async function handlePlayToggle() {
    if (!track) return;
    if (isPlaying) {
      await api.pauseTrack();
      isPlaying = false;
    } else {
      await api.playTrack(track.id, isCleanAudio, currentPosSecs);
      isPlaying = true;
    }
  }

  async function handleSeek(sec: number) {
    currentPosSecs = sec;
    try {
      await api.seekTrack(sec);
    } catch (e) {
      console.warn('Seek error:', e);
    }
  }

  // Seamless real-time A/B audio toggle
  async function handleEnhanceToggle() {
    isCleanAudio = !isCleanAudio;
    if (track) {
      try {
        await api.playTrack(track.id, isCleanAudio, currentPosSecs);
      } catch (e) {
        console.warn('Enhance switch error:', e);
      }
    }
  }

  async function handleFavoriteToggle() {
    if (!track) return;
    try {
      const fav = await api.toggleFavorite(track.id);
      track.is_favorite = fav;
    } catch (e) {
      console.error(e);
    }
  }

  async function triggerReprocess() {
    if (!track) return;
    isReprocessing = true;
    try {
      const updated = await api.processTrack(track.id, settings);
      track = updated;
    } catch (e) {
      console.error('Reprocess failed:', e);
    } finally {
      isReprocessing = false;
    }
  }

  async function handleSaveRename() {
    if (!track || !renameValue.trim()) return;
    try {
      await api.renameTrack(track.id, renameValue.trim());
      track.title = renameValue.trim();
      isRenaming = false;
      showMoreMenu = false;
    } catch (e) {
      console.error(e);
    }
  }

  async function handleOpenFolder() {
    if (!track) return;
    await api.openFolder(track.id);
    showMoreMenu = false;
  }

  async function handleDeleteTrack() {
    if (!track) return;
    await api.deleteTrack(track.id);
    showMoreMenu = false;
    onNavigateBack();
  }
</script>

<div class="panel-detail">
  <!-- Top Bar -->
  <header class="detail-topbar">
    <button class="btn-pill" onclick={onNavigateBack}>
      <ChevronLeft size={18} />
      <span>Back</span>
    </button>

    <div class="options-wrapper">
      <button
        class="icon-btn-more"
        onclick={() => (showMoreMenu = !showMoreMenu)}
        title="More options"
      >
        <MoreHorizontal size={20} color="rgba(255,255,255,0.85)" />
      </button>

      {#if showMoreMenu}
        <div class="more-menu">
          <button class="menu-item" onclick={() => (isRenaming = true)}>
            Rename Track
          </button>
          <button class="menu-item" onclick={handleOpenFolder}>
            Show in Files
          </button>
          <button class="menu-item menu-item-danger" onclick={handleDeleteTrack}>
            Delete Track
          </button>
        </div>
      {/if}
    </div>
  </header>

  <!-- Track Title Header -->
  <div class="track-header-title">
    {#if isRenaming}
      <div class="rename-box">
        <input
          type="text"
          class="rename-input"
          bind:value={renameValue}
          onkeydown={(e) => e.key === 'Enter' && handleSaveRename()}
        />
        <button class="btn-save" onclick={handleSaveRename}>Save</button>
      </div>
    {:else}
      <h1 class="track-name">{track?.title || 'Track 01'}</h1>
    {/if}
    <p class="track-meta">{track?.created_at || 'Mon, Oct 27, 2025 • 12:14'}</p>
  </div>

  <!-- Main Content Split (Scrubber Hero + Advanced Settings) -->
  <div class="detail-grid">
    <!-- Left / Center Section: Comparison Hero & Bottom Action Cards -->
    <div class="hero-and-actions">
      <!-- Split Waveform Scrubber with draggable divider -->
      <div class="scrubber-card">
        <SplitWaveformScrubber
          cleanWaveform={track?.clean_waveform || []}
          rawWaveform={track?.raw_waveform || []}
          durationSecs={track?.duration_secs || 134}
          {currentPosSecs}
          {isPlaying}
          {isCleanAudio}
          onPlayToggle={handlePlayToggle}
          onSeek={handleSeek}
        />
      </div>

      <!-- Bottom 4 Action Cards -->
      <div class="bottom-action-cards">
        <!-- 1. Enhance Card (Real-time A/B switch) -->
        <div
          class="action-card enhance-card"
          class:enhance-active={isCleanAudio}
          onclick={handleEnhanceToggle}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && handleEnhanceToggle()}
        >
          <div class="enhance-left">
            <Activity size={24} color={isCleanAudio ? 'var(--accent-lime)' : 'rgba(255,255,255,0.7)'} />
            <div class="card-texts">
              <span class="card-headline">Enhance</span>
              <span class="card-subheadline">
                {isCleanAudio ? 'Noise removal ON' : 'Noise removal OFF'}
              </span>
            </div>
          </div>
          <div class="toggle-switch" class:active={isCleanAudio}>
            <div class="toggle-knob"></div>
          </div>
        </div>

        <!-- 2. Favorite Card -->
        <div
          class="action-card favorite-card"
          onclick={handleFavoriteToggle}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && handleFavoriteToggle()}
        >
          <Star
            size={22}
            fill={track?.is_favorite ? 'var(--accent-lime)' : 'none'}
            color={track?.is_favorite ? 'var(--accent-lime)' : 'rgba(255,255,255,0.7)'}
          />
          <span class="card-headline">Favorite</span>
        </div>

        <!-- 3. Edit Card (Disabled "Soon") -->
        <div class="action-card edit-card disabled">
          <Scissors size={22} color="rgba(255,255,255,0.3)" />
          <div class="edit-text-group">
            <span class="card-headline disabled-text">Edit</span>
            <span class="soon-badge">Soon</span>
          </div>
        </div>

        <!-- 4. Share Card -->
        <div
          class="action-card share-card"
          onclick={handleOpenFolder}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && handleOpenFolder()}
        >
          <Upload size={22} color="rgba(255,255,255,0.7)" />
          <div class="card-texts">
            <span class="card-headline">Share</span>
            <span class="card-subheadline">Save / Export</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Right Sidebar: Advanced Settings -->
    <aside class="sidebar-settings card-dark">
      <h2 class="sidebar-heading">Advanced Settings</h2>

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
              triggerReprocess();
            }}
            role="switch"
            aria-checked={settings.conservative_bias}
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (settings.conservative_bias = !settings.conservative_bias)}
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
              triggerReprocess();
            }}
            role="switch"
            aria-checked={settings.declicker}
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (settings.declicker = !settings.declicker)}
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
              triggerReprocess();
            }}
            role="switch"
            aria-checked={settings.plosive_guard}
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && (settings.plosive_guard = !settings.plosive_guard)}
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
            step="0.05"
            class="lime-slider"
            bind:value={settings.harmonic_preservation}
            onchange={triggerReprocess}
          />
        </div>

        <!-- Control 5: AI Engine Model Dropdown -->
        <div class="setting-item-column">
          <span class="setting-name">AI Engine Model</span>
          <div class="select-wrapper">
            <select
              class="select-input"
              bind:value={settings.model_id}
              onchange={triggerReprocess}
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
          onclick={() => track && onOpenDiagnostics(track.id)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && track && onOpenDiagnostics(track.id)}
        >
          <div class="diag-left">
            <div class="diag-bars">
              <span class="bar bar-1"></span>
              <span class="bar bar-2"></span>
              <span class="bar bar-3"></span>
            </div>
            <div class="diag-texts">
              <span class="setting-name">Audio Diagnostics</span>
              <span class="setting-desc">View noise profile, levels and analysis</span>
            </div>
          </div>
          <ChevronRight size={18} color="rgba(255,255,255,0.4)" />
        </div>
      </div>
    </aside>
  </div>
</div>

<style>
  .panel-detail {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 20px 32px;
    background-color: var(--bg-app);
    overflow: hidden;
  }

  .detail-topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
    width: 100%;
  }

  .options-wrapper {
    position: relative;
  }

  .icon-btn-more {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: #16161A;
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .icon-btn-more:hover {
    background: #22222A;
  }

  .more-menu {
    position: absolute;
    top: 42px;
    right: 0;
    width: 160px;
    background: #1C1C22;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 12px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
    z-index: 50;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .menu-item {
    background: transparent;
    border: none;
    color: #FFFFFF;
    padding: 10px 14px;
    text-align: left;
    font-size: 13px;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .menu-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .menu-item-danger {
    color: #FF453A;
  }

  .track-header-title {
    margin-bottom: 20px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .track-name {
    font-family: var(--font-brand);
    font-size: 30px;
    font-weight: 700;
    color: #FFFFFF;
    letter-spacing: -0.5px;
  }

  .track-meta {
    font-size: 13.5px;
    color: var(--text-muted);
  }

  .rename-box {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .rename-input {
    background: #16161A;
    border: 1px solid var(--accent-lime);
    color: #FFFFFF;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 20px;
    font-weight: 600;
    outline: none;
  }

  .btn-save {
    background: var(--accent-lime);
    color: #000000;
    border: none;
    padding: 7px 16px;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
  }

  .detail-grid {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 20px;
    flex: 1;
    overflow: hidden;
  }

  .hero-and-actions {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 16px;
    overflow: hidden;
  }

  .scrubber-card {
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
  }

  .bottom-action-cards {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    width: 100%;
    margin-bottom: 8px;
  }

  .action-card {
    background: #16161A;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 14px 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .action-card:hover {
    background: #1C1C22;
    border-color: rgba(255, 255, 255, 0.16);
    transform: translateY(-2px);
  }

  .enhance-card.enhance-active {
    border-color: var(--accent-lime);
    box-shadow: 0 0 16px rgba(198, 255, 61, 0.18);
  }

  .enhance-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .card-texts {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .card-headline {
    font-size: 15px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .card-subheadline {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .favorite-card {
    justify-content: center;
    gap: 10px;
  }

  .edit-card {
    justify-content: center;
    gap: 10px;
    opacity: 0.6;
    cursor: not-allowed;
  }

  .edit-card:hover {
    transform: none;
    border-color: rgba(255, 255, 255, 0.08);
  }

  .edit-text-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .soon-badge {
    background: rgba(255, 255, 255, 0.12);
    color: var(--text-muted);
    font-size: 10.5px;
    padding: 2px 6px;
    border-radius: 6px;
    font-weight: 500;
  }

  .share-card {
    justify-content: center;
    gap: 12px;
  }

  /* Right Sidebar */
  .sidebar-settings {
    padding: 24px 22px;
    display: flex;
    flex-direction: column;
    gap: 18px;
    overflow-y: auto;
  }

  .sidebar-heading {
    font-family: var(--font-brand);
    font-size: 18px;
    font-weight: 700;
    color: #FFFFFF;
  }

  .settings-items-list {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .setting-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .setting-labels {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .setting-name {
    font-size: 13.5px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .setting-desc {
    font-size: 11.5px;
    color: var(--text-muted);
    line-height: 1.3;
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
    font-size: 13.5px;
    font-weight: 600;
    color: #FFFFFF;
  }

  .select-wrapper {
    position: relative;
    width: 100%;
  }

  .select-input {
    width: 100%;
    background: #0B0B0D;
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #FFFFFF;
    padding: 9px 12px;
    border-radius: 10px;
    font-size: 13px;
    appearance: none;
    outline: none;
    cursor: pointer;
  }

  :global(.select-chevron) {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
    color: rgba(255, 255, 255, 0.5);
  }

  .diagnostics-trigger-row {
    background: #0B0B0D;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 12px 14px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    transition: all 0.15s ease;
    margin-top: 4px;
  }

  .diagnostics-trigger-row:hover {
    border-color: rgba(255, 255, 255, 0.2);
    background: #141418;
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
    height: 18px;
  }

  .diag-bars .bar {
    width: 3px;
    background-color: #FFFFFF;
    border-radius: 9999px;
  }

  .diag-bars .bar-1 { height: 8px; }
  .diag-bars .bar-2 { height: 16px; }
  .diag-bars .bar-3 { height: 12px; }

  .diag-texts {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  @media (max-width: 820px) {
    .detail-grid {
      grid-template-columns: 1fr;
      overflow-y: auto;
    }
    .bottom-action-cards {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
