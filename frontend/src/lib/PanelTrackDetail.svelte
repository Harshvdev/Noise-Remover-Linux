<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from './api';
  import type { AdvancedSettingsDto, TrackMetadata } from './types';
  import SplitWaveformScrubber from './SplitWaveformScrubber.svelte';
  import AdvancedSettingsDrawer from './AdvancedSettingsDrawer.svelte';
  import {
    ChevronLeft,
    MoreHorizontal,
    Star,
    Scissors,
    Upload,
    Sliders,
    Edit3,
    Folder,
    Trash2,
  } from '@lucide/svelte';

  export let trackId: string;
  export let onNavigateBack: () => void;
  export let onOpenDiagnostics: (id: string) => void;

  let track: TrackMetadata | null = null;
  let isPlaying = false;
  let currentPosSecs = 0.0;
  let isCleanAudio = true;
  let isReprocessing = false;

  // Options Menu & Drawer state
  let showMoreMenu = false;
  let showAdvancedSettings = false;
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
  let animFrameId: number;
  let lastFrameTime = performance.now();
  let pendingSeekExpires = 0;

  function cleanDate(d: string | undefined): string {
    if (!d) return 'Mon, Oct 27, 2025 12:14';
    return d.replace('•', '').replace(/\s+/g, ' ').trim();
  }

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

  function updatePlayhead() {
    const now = performance.now();
    const dt = (now - lastFrameTime) / 1000;
    lastFrameTime = now;

    if (isPlaying && track && track.duration_secs > 0) {
      currentPosSecs = Math.min(track.duration_secs, currentPosSecs + dt);
      if (currentPosSecs >= track.duration_secs) {
        isPlaying = false;
      }
    }

    animFrameId = requestAnimationFrame(updatePlayhead);
  }

  onMount(() => {
    loadTrackData();
    lastFrameTime = performance.now();
    animFrameId = requestAnimationFrame(updatePlayhead);

    statusPollInterval = setInterval(async () => {
      try {
        const status = await api.getPlaybackStatus();
        if (status.track_id === trackId) {
          isPlaying = status.is_playing;
          isCleanAudio = status.is_clean;

          if (pendingSeekExpires > Date.now()) {
            const diff = Math.abs(status.position_seconds - currentPosSecs);
            if (diff <= 0.75) {
              pendingSeekExpires = 0;
              currentPosSecs = status.position_seconds;
            }
          } else {
            if (!isPlaying || Math.abs(status.position_seconds - currentPosSecs) > 0.25) {
              currentPosSecs = status.position_seconds;
            }
          }
        }
      } catch (e) {
        // Ignore fallback
      }
    }, 100);
  });

  onDestroy(() => {
    if (statusPollInterval) clearInterval(statusPollInterval);
    if (animFrameId) cancelAnimationFrame(animFrameId);
  });

  async function handlePlayToggle() {
    if (!track) return;
    if (isPlaying) {
      await api.pauseTrack();
      isPlaying = false;
    } else {
      lastFrameTime = performance.now();
      await api.playTrack(track.id, isCleanAudio, currentPosSecs);
      isPlaying = true;
    }
  }

  async function handleSeek(sec: number) {
    currentPosSecs = sec;
    lastFrameTime = performance.now();
    pendingSeekExpires = Date.now() + 800;
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

  function handleWindowClick(e: MouseEvent) {
    if (!showMoreMenu) return;
    const target = e.target as HTMLElement | null;
    if (!target?.closest('.more-card')) {
      showMoreMenu = false;
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && showMoreMenu) {
      showMoreMenu = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<div class="panel-detail">
  <div class="studio-wrapper">
    <!-- Top Bar -->
    <header class="detail-topbar">
      <button class="btn-nav" onclick={onNavigateBack}>
        <ChevronLeft size={18} />
        <span>Back</span>
      </button>
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
      <p class="track-meta">{cleanDate(track?.created_at)}</p>
    </div>

    <!-- Centered Studio Player Area -->
    <main class="player-stage">
      <!-- Waveform Stage Hero with Detached Pin & Hover-Activated Play Button -->
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

      <!-- Bottom 5 Action Cards -->
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
          <div class="card-top-row">
            <div class="enhance-bars-icon" class:active={isCleanAudio}>
              <span class="ebar eb1"></span>
              <span class="ebar eb2"></span>
              <span class="ebar eb3"></span>
              <span class="ebar eb4"></span>
              <span class="ebar eb5"></span>
            </div>
            <div class="toggle-switch" class:active={isCleanAudio}>
              <div class="toggle-knob"></div>
            </div>
          </div>
          <div class="card-bottom-info">
            <span class="card-headline">Enhance</span>
            <span class="card-subheadline">
              <span class="sub-full">{isCleanAudio ? 'Noise removal ON' : 'Noise removal OFF'}</span>
              <span class="sub-short">{isCleanAudio ? 'Active' : 'Off'}</span>
            </span>
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
          <div class="card-top-row">
            <Star
              size={24}
              fill={track?.is_favorite ? 'var(--accent-lime)' : 'none'}
              color={track?.is_favorite ? 'var(--accent-lime)' : 'rgba(255,255,255,0.75)'}
              strokeWidth={1.8}
            />
          </div>
          <div class="card-bottom-info">
            <span class="card-headline">Favorite</span>
          </div>
        </div>

        <!-- 3. Edit Card (Disabled "Soon") -->
        <div class="action-card edit-card disabled">
          <div class="card-top-row">
            <Scissors size={24} color="rgba(255,255,255,0.3)" strokeWidth={1.8} />
          </div>
          <div class="card-bottom-info">
            <div class="edit-headline-row">
              <span class="card-headline disabled-text">Edit</span>
              <span class="soon-badge">Soon</span>
            </div>
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
          <div class="card-top-row">
            <Upload size={24} color="rgba(255,255,255,0.75)" strokeWidth={1.8} />
          </div>
          <div class="card-bottom-info">
            <span class="card-headline">Share</span>
            <span class="card-subheadline">
              <span class="sub-full">Save / Export</span>
              <span class="sub-short">Export</span>
            </span>
          </div>
        </div>

        <!-- 5. More Card on the Very Right (Direct Grid Child) -->
        <div
          class="action-card more-card"
          class:more-active={showMoreMenu}
          onclick={(e) => {
            e.stopPropagation();
            showMoreMenu = !showMoreMenu;
          }}
          role="button"
          tabindex="0"
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              e.stopPropagation();
              showMoreMenu = !showMoreMenu;
            }
          }}
        >
          <div class="card-top-row">
            <MoreHorizontal size={24} color="rgba(255,255,255,0.85)" />
          </div>
          <div class="card-bottom-info">
            <span class="card-headline">More</span>
            <span class="card-subheadline">
              <span class="sub-full">Settings & Info</span>
              <span class="sub-short">Settings</span>
            </span>
          </div>

          {#if showMoreMenu}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="more-menu" onclick={(e) => e.stopPropagation()}>
              <button
                class="menu-item"
                onclick={(e) => {
                  e.stopPropagation();
                  showMoreMenu = false;
                  showAdvancedSettings = true;
                }}
              >
                <Sliders size={16} color="var(--accent-lime)" />
                <span>Advanced Settings</span>
              </button>

              <button
                class="menu-item"
                onclick={(e) => {
                  e.stopPropagation();
                  showMoreMenu = false;
                  isRenaming = true;
                }}
              >
                <Edit3 size={16} color="rgba(255,255,255,0.8)" />
                <span>Rename Track</span>
              </button>

              <button
                class="menu-item"
                onclick={(e) => {
                  e.stopPropagation();
                  showMoreMenu = false;
                  handleOpenFolder();
                }}
              >
                <Folder size={16} color="rgba(255,255,255,0.8)" />
                <span>Show in Files</span>
              </button>

              <div class="menu-divider"></div>

              <button
                class="menu-item menu-item-danger"
                onclick={(e) => {
                  e.stopPropagation();
                  showMoreMenu = false;
                  handleDeleteTrack();
                }}
              >
                <Trash2 size={16} color="#FF453A" />
                <span>Delete Track</span>
              </button>
            </div>
          {/if}
        </div>
      </div>
    </main>
  </div>

  <!-- Advanced Settings Drawer (Hidden behind 'More' card option) -->
  {#if showAdvancedSettings}
    <AdvancedSettingsDrawer
      bind:settings
      onClose={() => (showAdvancedSettings = false)}
      onSettingsChange={triggerReprocess}
      onOpenDiagnostics={() => track && onOpenDiagnostics(track.id)}
    />
  {/if}
</div>

<style>
  .panel-detail {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: clamp(10px, 1.8vh, 22px) clamp(10px, 2.2vw, 36px);
    background-color: var(--bg-app);
    overflow-y: auto;
    overflow-x: hidden;
    box-sizing: border-box;
  }

  /* Centered studio container matching ~1000px */
  .studio-wrapper {
    max-width: 1000px;
    width: 100%;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .detail-topbar {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    margin-bottom: clamp(10px, 1.8vh, 18px);
    width: 100%;
    flex-shrink: 0;
  }

  /* Squircle nav buttons */
  .btn-nav {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: #141417;
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #FFFFFF;
    padding: 8px 18px;
    border-radius: 14px;
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .btn-nav:hover {
    background: #1C1C20;
    border-color: rgba(255, 255, 255, 0.16);
    transform: translateY(-1px);
  }

  .btn-nav:active {
    transform: translateY(0);
  }

  .more-card {
    position: relative;
    overflow: visible !important;
  }

  .more-card.more-active {
    border-color: rgba(255, 255, 255, 0.25);
    background: #1C1C24;
    transform: none !important;
    z-index: 50;
  }

  .more-menu {
    position: absolute;
    bottom: calc(100% + 10px);
    right: 0;
    width: 210px;
    background: #18181E;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 14px;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.75);
    z-index: 60;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 6px;
    animation: menuFadeIn 0.16s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes menuFadeIn {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .menu-item {
    background: transparent;
    border: none;
    color: #FFFFFF;
    padding: 10px 14px;
    text-align: left;
    font-size: 13.5px;
    cursor: pointer;
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    transition: background 0.15s ease;
    font-weight: 500;
  }

  .menu-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .menu-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 4px 6px;
  }

  .menu-item-danger {
    color: #FF453A;
  }

  .track-header-title {
    margin-bottom: clamp(10px, 1.8vh, 18px);
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
  }

  .track-name {
    font-family: var(--font-brand);
    font-size: clamp(26px, 3.2vw, 34px);
    font-weight: 700;
    color: #FFFFFF;
    letter-spacing: -0.5px;
    line-height: 1.1;
  }

  .track-meta {
    font-size: 13.5px;
    color: rgba(255, 255, 255, 0.45);
    font-weight: 400;
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

  .player-stage {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 18px;
    flex: 1;
    min-height: 0;
  }

  .scrubber-card {
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-height: 0;
  }

  /* Bottom 5 Action Cards — Prominent studio row matching reference */
  .bottom-action-cards {
    max-width: 960px;
    width: 100%;
    margin: 0 auto 6px auto;
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 12px;
    flex-shrink: 0;
    box-sizing: border-box;
  }

  .action-card {
    background: #121216;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    padding: 16px 16px;
    min-height: 108px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 10px;
    cursor: pointer;
    transition: background 0.18s cubic-bezier(0.16, 1, 0.3, 1),
                border-color 0.18s cubic-bezier(0.16, 1, 0.3, 1),
                transform 0.18s cubic-bezier(0.16, 1, 0.3, 1);
    box-sizing: border-box;
    user-select: none;
    overflow: visible;
  }

  .action-card:hover:not(.disabled) {
    background: #18181E;
    border-color: rgba(255, 255, 255, 0.16);
    transform: translateY(-2px);
  }

  .action-card:active:not(.disabled) {
    transform: translateY(0);
  }

  .enhance-card.enhance-active {
    border: 1.5px solid var(--accent-lime);
    box-shadow: 0 0 18px rgba(198, 255, 61, 0.14);
    background: #101014;
  }

  .card-top-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    height: 26px;
    min-width: 0;
    flex-shrink: 0;
  }

  /* Responsive scalable icons */
  .action-card :global(svg) {
    width: 24px !important;
    height: 24px !important;
    flex-shrink: 0;
  }

  /* 5-bar equalizer icon in Enhance card */
  .enhance-bars-icon {
    display: flex;
    align-items: flex-end;
    gap: 2.5px;
    height: 24px;
    flex-shrink: 0;
  }

  .enhance-bars-icon .ebar {
    width: 3px;
    background-color: rgba(255, 255, 255, 0.4);
    border-radius: 9999px;
    transition: background-color 0.2s ease;
  }

  .enhance-bars-icon.active .ebar {
    background-color: var(--accent-lime);
  }

  .enhance-bars-icon .eb1 { height: 9px; }
  .enhance-bars-icon .eb2 { height: 17px; }
  .enhance-bars-icon .eb3 { height: 24px; }
  .enhance-bars-icon .eb4 { height: 16px; }
  .enhance-bars-icon .eb5 { height: 8px; }

  /* Compact scalable toggle switch */
  .toggle-switch {
    width: 40px;
    height: 23px;
    border-radius: 9999px;
    background: rgba(255, 255, 255, 0.18);
    position: relative;
    cursor: pointer;
    transition: background 0.2s ease;
    flex-shrink: 0;
  }

  .toggle-knob {
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: #FFFFFF;
    position: absolute;
    top: 3px;
    left: 3px;
    transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1), background 0.2s ease;
  }

  .toggle-switch.active {
    background: var(--accent-lime);
  }

  .toggle-switch.active .toggle-knob {
    transform: translateX(17px);
    background: #FFFFFF;
  }

  .card-bottom-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    overflow: hidden;
    flex-shrink: 0;
  }

  .card-headline {
    font-family: var(--font-brand);
    font-size: 16px;
    font-weight: 700;
    color: #FFFFFF;
    letter-spacing: -0.2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.2;
  }

  .card-subheadline {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
    font-weight: 400;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.2;
  }

  .sub-short {
    display: none;
  }

  @media (max-width: 820px) {
    .bottom-action-cards {
      gap: 8px;
    }
    .action-card {
      padding: 12px 10px;
      min-height: 92px;
      border-radius: 14px;
    }
    .card-headline {
      font-size: 14px;
    }
    .card-subheadline {
      font-size: 11px;
    }
    .action-card :global(svg) {
      width: 21px !important;
      height: 21px !important;
    }
    .toggle-switch {
      width: 34px;
      height: 20px;
    }
    .toggle-knob {
      width: 14px;
      height: 14px;
    }
    .toggle-switch.active .toggle-knob {
      transform: translateX(14px);
    }
  }

  @media (max-width: 640px) {
    .sub-full {
      display: none;
    }
    .sub-short {
      display: inline;
    }
    .bottom-action-cards {
      gap: 6px;
    }
    .action-card {
      padding: 10px 8px;
      min-height: 78px;
      border-radius: 12px;
    }
    .card-headline {
      font-size: 12.5px;
    }
    .card-subheadline {
      font-size: 10px;
    }
    .action-card :global(svg) {
      width: 19px !important;
      height: 19px !important;
    }
    .toggle-switch {
      width: 28px;
      height: 17px;
    }
    .toggle-knob {
      width: 12px;
      height: 12px;
      top: 2.5px;
      left: 2.5px;
    }
    .toggle-switch.active .toggle-knob {
      transform: translateX(11px);
    }
  }

  @media (max-width: 480px) {
    .bottom-action-cards {
      gap: 4px;
    }
    .action-card {
      padding: 7px 5px;
      min-height: 64px;
      border-radius: 10px;
    }
    .card-headline {
      font-size: 11px;
    }
    .card-subheadline {
      display: none;
    }
    .soon-badge {
      display: none;
    }
    .action-card :global(svg) {
      width: 17px !important;
      height: 17px !important;
    }
    .toggle-switch {
      width: 24px;
      height: 14px;
    }
    .toggle-knob {
      width: 10px;
      height: 10px;
      top: 2px;
      left: 2px;
    }
    .toggle-switch.active .toggle-knob {
      transform: translateX(10px);
    }
  }

  .edit-headline-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .disabled-text {
    color: rgba(255, 255, 255, 0.35);
  }

  .soon-badge {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.55);
    font-size: 10.5px;
    font-weight: 500;
    padding: 2px 7px;
    border-radius: 9999px;
    letter-spacing: 0.2px;
    flex-shrink: 0;
  }

  .edit-card.disabled {
    cursor: default;
    opacity: 0.7;
  }

  @media (max-height: 720px) {
    .panel-detail {
      padding: 10px 18px;
    }
    .track-header-title {
      margin-bottom: 4px;
    }
    .player-stage {
      gap: 12px;
    }
    .action-card {
      min-height: 98px;
      padding: 12px 14px;
    }
  }
</style>
