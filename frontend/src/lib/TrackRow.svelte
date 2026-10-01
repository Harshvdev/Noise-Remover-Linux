<script lang="ts">
  import type { TrackMetadata } from './types';
  import { Play, Pause, Star, Trash2 } from '@lucide/svelte';

  export let track: TrackMetadata;
  export let isPlaying = false;
  export let currentPosSecs = 0;
  export let onPlayToggle: (e: MouseEvent) => void;
  export let onFavoriteToggle: (e: MouseEvent) => void;
  export let onDelete: (e: MouseEvent) => void;
  export let onOpenDetail: () => void;

  function formatTime(s: number): string {
    const mins = Math.floor(s / 60);
    const secs = Math.floor(s % 60);
    return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }

  $: playProgress = isPlaying && track.duration_secs > 0
    ? Math.min(1.0, currentPosSecs / track.duration_secs)
    : 0;

  $: activeWave = track.clean_waveform.length > 0 ? track.clean_waveform : track.raw_waveform;
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="track-row"
  class:is-active-playing={isPlaying}
  onclick={onOpenDetail}
>
  <!-- Play/Pause Button -->
  <button
    class="track-play-btn"
    class:btn-active={isPlaying}
    onclick={onPlayToggle}
    title={isPlaying ? 'Pause' : 'Play'}
  >
    {#if isPlaying}
      <Pause size={17} fill="#0B0B0D" color="#0B0B0D" />
    {:else}
      <Play size={17} fill="#FFFFFF" color="#FFFFFF" />
    {/if}
  </button>

  <!-- Track Info -->
  <div class="track-info">
    <span class="track-title">{track.title}</span>
    <span class="track-date">{track.created_at}</span>
  </div>

  <!-- Waveform Scrubber -->
  <div class="track-waveform-wrapper">
    <div class="waveform-bars">
      {#each activeWave.slice(0, 90) as peak, idx}
        {@const barProgress = idx / 90}
        {@const isPast = isPlaying && barProgress <= playProgress}
        <span
          class="wave-bar"
          class:is-past={isPast}
          class:is-idle={!isPlaying}
          style="height: {Math.max(3, peak * 20)}px;"
        ></span>
      {/each}
    </div>

    <!-- Playhead Needle Line (shown on playing track) -->
    {#if isPlaying}
      <div class="playhead-needle" style="left: {playProgress * 100}%;">
        <div class="playhead-dot"></div>
      </div>
    {/if}
  </div>

  <!-- Duration -->
  <div class="track-duration tabular-nums">
    {#if isPlaying}
      {formatTime(currentPosSecs)} / {track.formatted_duration}
    {:else}
      00:00 / {track.formatted_duration}
    {/if}
  </div>

  <!-- Actions -->
  <div class="track-actions">
    <button
      class="action-btn star-btn"
      class:is-fav={track.is_favorite}
      onclick={onFavoriteToggle}
      title={track.is_favorite ? 'Unfavorite' : 'Favorite'}
    >
      <Star
        size={18}
        fill={track.is_favorite ? 'var(--accent-lime)' : 'none'}
        color={track.is_favorite ? 'var(--accent-lime)' : 'rgba(255, 255, 255, 0.45)'}
      />
    </button>

    <button
      class="action-btn delete-btn"
      onclick={onDelete}
      title="Delete Track"
    >
      <Trash2 size={17} color="rgba(255, 255, 255, 0.45)" />
    </button>
  </div>
</div>

<style>
  .track-row {
    width: 100%;
    height: 52px;
    background: #16161A;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 12px;
    display: flex;
    align-items: center;
    padding: 0 16px;
    gap: 14px;
    cursor: pointer;
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
    position: relative;
  }

  .track-row:hover {
    background: #1C1C22;
    border-color: rgba(255, 255, 255, 0.14);
    transform: translateY(-1px);
  }

  .track-row.is-active-playing {
    border-color: var(--accent-lime);
    box-shadow: 0 0 14px rgba(198, 255, 61, 0.15);
  }

  .track-play-btn {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: #25252C;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: all 0.15s ease;
  }

  .track-play-btn:hover {
    transform: scale(1.08);
  }

  .track-play-btn.btn-active {
    background: var(--accent-lime);
    box-shadow: 0 0 12px var(--accent-lime-glow);
  }

  .track-info {
    display: flex;
    flex-direction: column;
    width: 170px;
    flex-shrink: 0;
    gap: 3px;
  }

  .track-title {
    font-size: 14.5px;
    font-weight: 600;
    color: #FFFFFF;
    letter-spacing: -0.2px;
  }

  .track-date {
    font-size: 12px;
    color: var(--text-muted);
  }

  .track-waveform-wrapper {
    flex: 1;
    height: 32px;
    position: relative;
    display: flex;
    align-items: center;
    overflow: hidden;
  }

  .waveform-bars {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 2.5px;
  }

  .wave-bar {
    flex: 1;
    background-color: #383842;
    border-radius: 9999px;
    transition: background-color 0.12s ease;
  }

  .wave-bar.is-past {
    background-color: var(--accent-lime);
    box-shadow: 0 0 6px rgba(198, 255, 61, 0.4);
  }

  .playhead-needle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background-color: #FFFFFF;
    pointer-events: none;
    display: flex;
    justify-content: center;
  }

  .playhead-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background-color: #FFFFFF;
    position: absolute;
    top: -2px;
  }

  .track-duration {
    font-size: 12.5px;
    color: var(--text-muted);
    width: 100px;
    text-align: right;
    flex-shrink: 0;
  }

  .track-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .action-btn {
    background: transparent;
    border: none;
    padding: 6px;
    border-radius: 6px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .delete-btn:hover :global(svg) {
    color: #FF453A !important;
  }
</style>
