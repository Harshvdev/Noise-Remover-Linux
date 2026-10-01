<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { TrackMetadata } from './types';
  import { Play, Pause, Star, X } from '@lucide/svelte';

  export let track: TrackMetadata;
  export let isPlaying = false;
  export let currentPosSecs = 0;
  export let onPlayToggle: (e: MouseEvent) => void;
  export let onFavoriteToggle: (e: MouseEvent) => void;
  export let onClose: (e: MouseEvent) => void;
  export let onOpenDetail: () => void;

  let nowSecs = Math.floor(Date.now() / 1000);
  let timeTicker: ReturnType<typeof setInterval>;

  onMount(() => {
    timeTicker = setInterval(() => {
      nowSecs = Math.floor(Date.now() / 1000);
    }, 10000);
  });

  onDestroy(() => {
    if (timeTicker) clearInterval(timeTicker);
  });

  // Calculate dynamic relative time passed since the recording was made
  function getTimeAgo(timestamp: number, _now: number): string {
    if (!timestamp) return 'Just now';
    const diff = Math.max(0, _now - timestamp);

    if (diff < 45) {
      return 'Just now';
    } else if (diff < 90) {
      return '1m ago';
    } else if (diff < 3600) {
      return `${Math.floor(diff / 60)}m ago`;
    } else if (diff < 86400) {
      return `${Math.floor(diff / 3600)}h ago`;
    } else if (diff < 604800) {
      return `${Math.floor(diff / 86400)}d ago`;
    } else {
      return new Date(timestamp * 1000).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
      });
    }
  }

  // Format duration with tenths: 00:12.4 matching mockup
  function formatDurationWithTenths(secs: number): string {
    if (!secs || isNaN(secs)) return '00:12.4';
    const mins = Math.floor(secs / 60).toString().padStart(2, '0');
    const s = Math.floor(secs % 60).toString().padStart(2, '0');
    const tenths = Math.floor((secs % 1) * 10);
    return `${mins}:${s}.${tenths}`;
  }

  function formatPlaybackPos(secs: number): string {
    const mins = Math.floor(secs / 60).toString().padStart(2, '0');
    const s = Math.floor(secs % 60).toString().padStart(2, '0');
    return `${mins}:${s}`;
  }

  // Resample recorded audio waveform into exactly 32 bars shaped according to real recorded audio
  const BAR_COUNT = 32;
  function computeResampledBars(t: TrackMetadata): number[] {
    const raw = (t.clean_waveform && t.clean_waveform.length > 0)
      ? t.clean_waveform
      : (t.raw_waveform && t.raw_waveform.length > 0)
        ? t.raw_waveform
        : [];

    if (raw.length === 0) {
      // Default subtle voice envelope if no points recorded
      return Array.from({ length: BAR_COUNT }, (_, i) => {
        const x = i / BAR_COUNT;
        const env = Math.sin(x * Math.PI);
        return Math.max(3, Math.round(env * 18));
      });
    }

    const peaks: number[] = [];
    let overallMax = 0.001;

    for (let i = 0; i < BAR_COUNT; i++) {
      const startIdx = Math.floor((i * raw.length) / BAR_COUNT);
      const endIdx = Math.max(startIdx + 1, Math.floor(((i + 1) * raw.length) / BAR_COUNT));
      let peak = 0;
      for (let j = startIdx; j < endIdx && j < raw.length; j++) {
        if (raw[j] > peak) peak = raw[j];
      }
      peaks.push(peak);
      if (peak > overallMax) overallMax = peak;
    }

    return peaks.map((p) => {
      const norm = Math.min(1.0, p / overallMax);
      // Min 3px dot up to 21px peak height
      return Math.max(3, Math.round(3 + norm * 18));
    });
  }

  $: timeAgoStr = getTimeAgo(track.timestamp, nowSecs);
  $: durationTenthsStr = formatDurationWithTenths(track.duration_secs);
  $: miniBars = computeResampledBars(track);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="new-recording-card" onclick={onOpenDetail}>
  <div class="card-header">
    <div class="title-group">
      <span class="card-title">New Recording</span>
      <span class="card-meta">{timeAgoStr} • {durationTenthsStr}</span>
    </div>
    <button class="icon-btn-close" onclick={onClose} title="Dismiss">
      <X size={15} />
    </button>
  </div>

  <div class="player-row">
    <!-- Play / Pause Circle Button -->
    <button class="play-btn" onclick={onPlayToggle} title={isPlaying ? 'Pause' : 'Play'}>
      {#if isPlaying}
        <Pause size={13} fill="#FFFFFF" color="#FFFFFF" />
      {:else}
        <Play size={13} fill="#FFFFFF" color="#FFFFFF" style="margin-left: 2px;" />
      {/if}
    </button>

    <!-- Mini Waveform Shaped According to Recorded Audio -->
    <div class="mini-waveform">
      {#each miniBars as barHeight}
        <span class="mini-bar" style="height: {barHeight}px;"></span>
      {/each}
    </div>

    <!-- Playback / Duration Time Label -->
    <span class="time-label tabular-nums">
      {isPlaying ? formatPlaybackPos(currentPosSecs) : '00:00'} / {track.formatted_duration || '00:12'}
    </span>

    <!-- Star / Favorite Button matching panel-1.png box -->
    <button
      class="star-box-btn"
      class:is-fav={track.is_favorite}
      onclick={onFavoriteToggle}
      title="Favorite"
    >
      <Star
        size={15}
        fill={track.is_favorite ? 'var(--accent-lime)' : 'none'}
        color={track.is_favorite ? 'var(--accent-lime)' : 'rgba(255,255,255,0.75)'}
        strokeWidth={1.8}
      />
    </button>
  </div>
</div>

<style>
  .new-recording-card {
    background: #111114;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
    padding: 15px 16px;
    width: 326px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
    cursor: pointer;
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .new-recording-card:hover {
    border-color: rgba(255, 255, 255, 0.14);
    transform: translateY(-2px);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 12px;
  }

  .title-group {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .card-title {
    font-size: 14.5px;
    font-weight: 600;
    color: #FFFFFF;
    letter-spacing: -0.2px;
  }

  .card-meta {
    font-size: 12.5px;
    color: rgba(255, 255, 255, 0.45);
    font-weight: 400;
  }

  .icon-btn-close {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.45);
    cursor: pointer;
    padding: 4px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s ease;
  }

  .icon-btn-close:hover {
    color: #FFFFFF;
  }

  .player-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .play-btn {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: #222227;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: #FFFFFF;
    flex-shrink: 0;
    transition: background 0.15s ease, transform 0.12s ease;
  }

  .play-btn:hover {
    background: #2E2E36;
    transform: scale(1.05);
  }

  .mini-waveform {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    height: 26px;
    flex: 1;
    overflow: hidden;
  }

  .mini-bar {
    width: 2.5px;
    background-color: var(--accent-lime);
    border-radius: 9999px;
    transition: height 0.1s ease;
  }

  .time-label {
    font-size: 11.5px;
    color: rgba(255, 255, 255, 0.45);
    flex-shrink: 0;
  }

  .star-box-btn {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: #18181D;
    border: 1px solid rgba(255, 255, 255, 0.08);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: background-color 0.15s ease, border-color 0.15s ease, transform 0.12s ease;
  }

  .star-box-btn:hover {
    background: #22222A;
    border-color: rgba(255, 255, 255, 0.16);
    transform: scale(1.05);
  }

  .star-box-btn.is-fav {
    border-color: rgba(198, 255, 61, 0.3);
  }
</style>
