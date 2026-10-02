<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from './api';
  import type { TrackMetadata } from './types';
  import WaveformLive from './WaveformLive.svelte';
  import RecordButton from './RecordButton.svelte';
  import CountdownOverlay from './CountdownOverlay.svelte';
  import NewRecordingCard from './NewRecordingCard.svelte';
  import { Settings, List } from '@lucide/svelte';

  export let onNavigateRecordings: () => void;
  export let onNavigateTrackDetail: (trackId: string) => void;
  export let onOpenSettings: () => void;

  let isRecording = false;
  let isCalibrating = false;
  let isCountingDown = false;
  let micLevel = 0.0;
  let liveSpectrum: number[] = new Array(128).fill(0);
  let elapsedSecs = 0.0;
  let timerInterval: ReturnType<typeof setInterval>;
  let statsPollInterval: ReturnType<typeof setInterval>;

  let latestRecording: TrackMetadata | null = null;
  let isLatestPlaying = false;
  let latestPlaybackPos = 0.0;
  let playbackTicker: ReturnType<typeof setInterval>;
  let isDesktop = true;

  onMount(async () => {
    if (typeof navigator !== 'undefined') {
      isDesktop = !/Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent);
    }
    // Poll mic stats periodically for real-time responsiveness
    statsPollInterval = setInterval(async () => {
      try {
        const stats = await api.getMicStats();
        // Convert peak_dbfs (-55 to -10) to 0.0 - 1.0 linear level
        const norm = Math.max(0, (stats.peak_dbfs + 55) / 45);
        micLevel = Math.min(1.0, norm);
        if (stats.spectrum && stats.spectrum.length === 128) {
          liveSpectrum = stats.spectrum;
        }
      } catch (e) {
        // fallback in preview when backend not connected
        micLevel = 0.0;
      }
    }, 35);
  });

  onDestroy(() => {
    if (timerInterval) clearInterval(timerInterval);
    if (statsPollInterval) clearInterval(statsPollInterval);
    if (playbackTicker) clearInterval(playbackTicker);
  });

  function formatTimer(secs: number): string {
    const mins = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.floor((secs % 1) * 10);
    const mStr = String(mins).padStart(2, '0');
    const sStr = String(s).padStart(2, '0');
    return `${mStr}:${sStr}.${ms}`;
  }

  async function handleRecordClick() {
    if (isRecording) {
      // Stop recording
      if (timerInterval) clearInterval(timerInterval);
      isRecording = false;
      try {
        const track = await api.stopRecording();
        latestRecording = track;
      } catch (e) {
        console.error('Error stopping recording:', e);
      }
    } else {
      // Start 3-second calibration countdown
      try {
        await api.prepareRecordingSession();
      } catch (e) {
        console.warn('Prepare session error:', e);
      }
      isCountingDown = true;
    }
  }

  async function handleCountdownComplete() {
    isCountingDown = false;
    isCalibrating = false;
    isRecording = true;
    elapsedSecs = 0.0;

    try {
      await api.startRecording();
    } catch (e) {
      console.warn('Start recording error:', e);
    }

    timerInterval = setInterval(() => {
      elapsedSecs += 0.1;
    }, 100);
  }

  function handleCountdownCancel() {
    isCountingDown = false;
    isCalibrating = false;
    isRecording = false;
  }

  async function handleLatestPlayToggle(e: MouseEvent) {
    e.stopPropagation();
    if (!latestRecording) return;
    if (isLatestPlaying) {
      await api.pauseTrack();
      isLatestPlaying = false;
      if (playbackTicker) clearInterval(playbackTicker);
    } else {
      await api.playTrack(latestRecording.id, true);
      isLatestPlaying = true;
      latestPlaybackPos = 0;
      if (playbackTicker) clearInterval(playbackTicker);
      playbackTicker = setInterval(() => {
        if (!isLatestPlaying || !latestRecording) {
          if (playbackTicker) clearInterval(playbackTicker);
          return;
        }
        latestPlaybackPos += 0.2;
        if (latestPlaybackPos >= latestRecording.duration_secs) {
          latestPlaybackPos = 0;
          isLatestPlaying = false;
          if (playbackTicker) clearInterval(playbackTicker);
        }
      }, 200);
    }
  }

  async function handleLatestFavorite(e: MouseEvent) {
    e.stopPropagation();
    if (!latestRecording) return;
    try {
      const fav = await api.toggleFavorite(latestRecording.id);
      latestRecording.is_favorite = fav;
    } catch (e) {
      console.error(e);
    }
  }

  function handleDismissLatest(e: MouseEvent) {
    e.stopPropagation();
    latestRecording = null;
  }
</script>

<div class="panel-home">
  <!-- Top Navigation Header -->
  <header class="home-header" class:mobile-header={!isDesktop}>
    {#if isDesktop}
      <div class="brand-group">
        <!-- Waveform Icon (Brand logo) -->
        <div class="header-logo">
          <span class="bar bar-1"></span>
          <span class="bar bar-2"></span>
          <span class="bar bar-3"></span>
          <span class="bar bar-4"></span>
          <span class="bar bar-5"></span>
        </div>
        <div class="brand-text">
          <h1 class="brand-title">Voice Cleaner</h1>
          <p class="brand-subtitle">Clear Voice. Pure Sound.</p>
        </div>
      </div>
    {:else}
      <div class="brand-group mobile-brand">
        <div class="header-logo">
          <span class="bar bar-1"></span>
          <span class="bar bar-2"></span>
          <span class="bar bar-3"></span>
          <span class="bar bar-4"></span>
          <span class="bar bar-5"></span>
        </div>
        <h1 class="brand-title-mobile">Voice Cleaner</h1>
      </div>
    {/if}

    <!-- Settings Button -->
    <button class="btn-nav btn-settings" onclick={onOpenSettings} aria-label="Settings">
      <Settings size={18} color="var(--text-main)" />
      {#if isDesktop}<span>Settings</span>{/if}
    </button>
  </header>

  <!-- Center Hero Area -->
  <main class="home-center">
    <!-- Horizontal Live Waveform with Noise Cloud -->
    <div class="live-wave-wrapper">
      <WaveformLive {isRecording} {isCalibrating} level={micLevel} spectrum={liveSpectrum} />
    </div>

    <!-- Big Timer -->
    <div class="timer-display tabular-nums">
      {formatTimer(elapsedSecs)}
    </div>

    <!-- Red Circular Record Button with Concentric Outer Level Ring -->
    <RecordButton
      {isRecording}
      {isCalibrating}
      onToggle={handleRecordClick}
    />
  </main>

  <!-- Bottom Navigation Row -->
  <footer class="home-footer">
    <div class="footer-left">
      {#if latestRecording}
        <NewRecordingCard
          track={latestRecording}
          isPlaying={isLatestPlaying}
          currentPosSecs={latestPlaybackPos}
          onPlayToggle={handleLatestPlayToggle}
          onFavoriteToggle={handleLatestFavorite}
          onClose={handleDismissLatest}
          onOpenDetail={() => onNavigateTrackDetail(latestRecording!.id)}
        />
      {/if}
    </div>

    <div class="footer-right">
      <!-- Recordings List Button with Accent List Icon -->
      <button class="btn-nav btn-recordings-list" onclick={onNavigateRecordings}>
        <List size={18} color="var(--accent-lime)" strokeWidth={2.4} />
        <span>Recordings List</span>
      </button>
    </div>
  </footer>

  <!-- 3-Second Calibration Countdown Overlay -->
  {#if isCountingDown}
    <CountdownOverlay
      onComplete={handleCountdownComplete}
      onCancel={handleCountdownCancel}
    />
  {/if}
</div>

<style>
  .panel-home {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: clamp(14px, 2.5vh, 24px) clamp(16px, 3.5vw, 36px);
    background-color: var(--bg-app);
    position: relative;
    overflow: hidden;
    box-sizing: border-box;
    overscroll-behavior: none;
    touch-action: manipulation;
  }

  .home-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    z-index: 10;
    flex-shrink: 0;
  }

  .brand-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  /* Brand Logo bars left untouched as requested */
  .header-logo {
    display: flex;
    align-items: center;
    gap: 3.5px;
    height: 32px;
  }

  .header-logo .bar {
    width: 4px;
    background-color: var(--accent-lime);
    border-radius: 9999px;
  }

  .header-logo .bar-1 { height: 14px; }
  .header-logo .bar-2 { height: 26px; }
  .header-logo .bar-3 { height: 32px; }
  .header-logo .bar-4 { height: 22px; }
  .header-logo .bar-5 { height: 12px; }

  .brand-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .brand-title {
    font-family: var(--font-brand);
    font-size: 24px;
    font-weight: 700;
    color: var(--text-main);
    letter-spacing: -0.5px;
    line-height: 1.1;
  }

  .brand-subtitle {
    font-size: 13px;
    color: var(--text-muted);
    font-weight: 400;
  }

  /* Squircle nav buttons matching panel-1.png */
  .btn-nav {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    color: var(--text-main);
    padding: 8px 16px;
    border-radius: 14px;
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: var(--shadow-card);
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
    flex-shrink: 0;
  }

  .btn-nav:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-medium);
    transform: translateY(-1px);
  }

  .btn-nav:active {
    transform: translateY(0);
  }

  .home-center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-height: 0;
    gap: clamp(8px, 2vh, 18px);
  }

  .live-wave-wrapper {
    width: 100%;
    max-width: 680px;
    display: flex;
    justify-content: center;
    flex-shrink: 1;
    min-height: 0;
  }

  .timer-display {
    font-family: var(--font-brand);
    font-size: clamp(38px, 9vh, 80px);
    font-weight: 700;
    color: var(--text-main);
    letter-spacing: -1.5px;
    line-height: 1;
    margin: 0;
    flex-shrink: 0;
  }

  .home-footer {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    width: 100%;
    z-index: 10;
    flex-shrink: 0;
  }

  .footer-left {
    min-height: 72px;
    display: flex;
    align-items: flex-end;
    max-width: 100%;
  }

  .footer-right {
    display: flex;
    align-items: flex-end;
  }

  /* Portrait and Mobile Adaptations */
  @media (max-width: 640px), (orientation: portrait) {
    .panel-home {
      padding-top: max(clamp(16px, 3vh, 28px), calc(var(--safe-top, 0px) + 14px));
      padding-bottom: max(clamp(20px, 3.5vh, 32px), calc(var(--safe-bottom, 0px) + 20px));
      padding-left: max(16px, calc(var(--safe-left, 0px) + 16px));
      padding-right: max(16px, calc(var(--safe-right, 0px) + 16px));
    }
    .brand-group.mobile-brand {
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .brand-title-mobile {
      font-family: var(--font-brand);
      font-size: 19px;
      font-weight: 700;
      color: var(--text-main);
      letter-spacing: -0.4px;
    }
    .home-header {
      margin-bottom: 4px;
    }
    .home-center {
      justify-content: center;
      gap: clamp(16px, 3vh, 32px);
    }
    .btn-settings {
      padding: 9px 11px;
      border-radius: 12px;
    }
    .home-footer {
      flex-direction: column;
      align-items: center;
      gap: 12px;
    }
    .footer-left {
      width: 100%;
      min-height: auto;
      justify-content: center;
    }
    .footer-right {
      width: 100%;
      justify-content: center;
      display: flex;
    }
    .btn-recordings-list {
      padding: 10px 24px;
      font-size: 13.5px;
      border-radius: 9999px;
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
    }
  }

  @media (orientation: landscape) and (max-height: 540px) {
    .panel-home {
      padding: 8px 18px;
    }
    .timer-display {
      font-size: clamp(32px, 8vh, 46px);
    }
    .home-center {
      gap: 6px;
    }
    .footer-left {
      min-height: auto;
    }
    .header-logo {
      height: 24px;
    }
    .brand-title {
      font-size: 18px;
    }
    .brand-subtitle {
      display: none;
    }
  }
</style>
