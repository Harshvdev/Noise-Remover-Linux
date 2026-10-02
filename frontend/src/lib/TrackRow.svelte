<script lang="ts">
  import type { TrackMetadata } from './types';
  import { Play, Pause, Star, Trash2 } from '@lucide/svelte';

  export let track: TrackMetadata;
  export let isPlaying = false;
  export let isCurrentTrack = false;
  export let currentPosSecs = 0;
  export let onPlayToggle: (e: MouseEvent) => void;
  export let onFavoriteToggle: (e: MouseEvent) => void;
  export let onDelete: (e: MouseEvent) => void;
  export let onOpenDetail: () => void;
  export let onSeek: ((sec: number, isFinal: boolean) => void) | undefined = undefined;

  let waveformWrapperEl: HTMLDivElement;
  let isDragging = false;
  let dragSecs = 0;

  function formatTime(s: number): string {
    const mins = Math.floor(s / 60);
    const secs = Math.floor(s % 60);
    return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }

  // Active play progress: while dragging follow dragSecs, otherwise immediately follow currentPosSecs
  $: effectivePosSecs = isDragging ? dragSecs : currentPosSecs;

  $: playProgress = isCurrentTrack && track.duration_secs > 0
    ? Math.min(1.0, Math.max(0, effectivePosSecs / track.duration_secs))
    : 0;

  $: activeWave = (track.clean_waveform && track.clean_waveform.length > 0)
    ? track.clean_waveform
    : (track.raw_waveform && track.raw_waveform.length > 0)
      ? track.raw_waveform
      : [];

  // Dynamically calculate number of bars based on audio duration.
  // Longer audio tracks get more bars that squeeze into thinner lines.
  function computeNumBars(durationSecs: number): number {
    if (!durationSecs || durationSecs <= 0) return 75;
    // Short tracks (~10-45s) -> 55-70 bars
    // Medium tracks (~60-140s) -> 85-115 bars
    // Long tracks (~180-300s+) -> 135-175 bars (thin and squeezed)
    const count = Math.round(55 + Math.sqrt(durationSecs) * 6.2);
    return Math.min(185, Math.max(55, count));
  }

  function resampleWaveform(wave: number[], targetCount: number): number[] {
    if (!wave || wave.length === 0) {
      return Array.from({ length: targetCount }, () => 0.1);
    }
    const result: number[] = [];
    let minVal = Infinity;
    let maxVal = -Infinity;

    for (let i = 0; i < targetCount; i++) {
      const start = Math.floor((i * wave.length) / targetCount);
      const end = Math.max(start + 1, Math.floor(((i + 1) * wave.length) / targetCount));
      let peak = 0;
      for (let j = start; j < end && j < wave.length; j++) {
        if (wave[j] > peak) peak = wave[j];
      }
      result.push(peak);
      if (peak < minVal) minVal = peak;
      if (peak > maxVal) maxVal = peak;
    }

    const range = maxVal - minVal;
    if (range < 0.04) {
      // Flat or low variation audio: clean baseline dots (3px)
      return Array.from({ length: targetCount }, () => 0.1);
    }

    // Dynamic contrast scaling with power curve (1.2)
    // Low baseline audio -> 0.10 (3px dot)
    // Voice peaks -> 1.0 (24px tall)
    return result.map((p) => {
      const norm = Math.max(0, Math.min(1, (p - minVal) / range));
      const curved = Math.pow(norm, 1.2);
      return 0.10 + curved * 0.90;
    });
  }

  $: numBars = computeNumBars(track.duration_secs);
  $: displayBars = resampleWaveform(activeWave, numBars);
  $: gapPx = numBars > 140 ? 0.8 : numBars > 105 ? 1.2 : numBars > 80 ? 1.6 : 2.2;
  $: maxBarWidth = numBars > 140 ? '1.8px' : numBars > 105 ? '2.4px' : numBars > 80 ? '3.0px' : '3.8px';

  function getSecFromClientX(clientX: number): number {
    if (!waveformWrapperEl || track.duration_secs <= 0) return 0;
    const rect = waveformWrapperEl.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
    const fraction = rect.width > 0 ? x / rect.width : 0;
    return fraction * track.duration_secs;
  }

  let justScrubbed = false;

  function handleRowClick(e: MouseEvent) {
    if (isDragging || justScrubbed) {
      e.stopPropagation();
      e.preventDefault();
      return;
    }
    onOpenDetail();
  }

  function handleWaveformMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    e.stopPropagation(); // Do not trigger onOpenDetail row click
    isDragging = true;
    justScrubbed = true;
    const sec = getSecFromClientX(e.clientX);
    dragSecs = sec;
    onSeek?.(sec, false);

    const onMouseMove = (moveEv: MouseEvent) => {
      if (!isDragging) return;
      moveEv.preventDefault();
      const s = getSecFromClientX(moveEv.clientX);
      dragSecs = s;
      onSeek?.(s, false);
    };

    const onMouseUp = (upEv: MouseEvent) => {
      if (!isDragging) return;
      upEv.stopPropagation();
      isDragging = false;
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
      const s = getSecFromClientX(upEv.clientX);
      dragSecs = s;
      onSeek?.(s, true);
      setTimeout(() => {
        justScrubbed = false;
      }, 150);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="track-row"
  class:is-active-playing={isCurrentTrack}
  onclick={handleRowClick}
>
  <!-- Play/Pause Circular Button -->
  <button
    class="track-play-btn"
    class:btn-active={isPlaying && isCurrentTrack}
    onclick={onPlayToggle}
    title={isPlaying && isCurrentTrack ? 'Pause' : 'Play'}
  >
    {#if isPlaying && isCurrentTrack}
      <Pause size={15} fill="#000000" color="#000000" />
    {:else}
      <Play size={13} fill="#FFFFFF" color="#FFFFFF" style="margin-left: 2px;" />
    {/if}
  </button>

  <!-- Track Title & Date -->
  <div class="track-info">
    <span class="track-title">{track.title}</span>
    <span class="track-date">{track.created_at}</span>
  </div>

  <!-- Interactive Waveform Scrubber & Movable Pin -->
  <div
    class="track-waveform-wrapper"
    class:is-scrubbing={isDragging}
    bind:this={waveformWrapperEl}
    onmousedown={handleWaveformMouseDown}
    onclick={(e) => e.stopPropagation()}
    role="slider"
    tabindex="0"
    aria-label="Seek track position"
    aria-valuenow={effectivePosSecs}
    aria-valuemin={0}
    aria-valuemax={track.duration_secs}
  >
    <div class="waveform-bars" style="gap: {gapPx}px;">
      {#each displayBars as peak, idx}
        {@const barProgress = idx / numBars}
        {@const isPast = isCurrentTrack && barProgress <= playProgress}
        <span
          class="wave-bar"
          class:is-past={isPast}
          class:is-idle={!isCurrentTrack}
          style="
            height: {Math.max(3, Math.round(peak * 24))}px;
            max-width: {maxBarWidth};
          "
        ></span>
      {/each}
    </div>

    <!-- Playhead Needle Line with circular top bead on active track (movable pin!) -->
    {#if isCurrentTrack}
      <div
        class="playhead-needle"
        class:is-dragging={isDragging}
        style="left: {playProgress * 100}%;"
      >
        <div class="playhead-dot"></div>
      </div>
    {/if}
  </div>

  <!-- Duration Display (e.g. 00:42 / 02:14 or 00:00 / 01:36) -->
  <div class="track-duration tabular-nums">
    {#if isCurrentTrack}
      {formatTime(effectivePosSecs)} / {track.formatted_duration}
    {:else}
      00:00 / {track.formatted_duration}
    {/if}
  </div>

  <!-- Action Buttons: Dark Squircle Boxes matching panel-2.png -->
  <div class="track-actions">
    <!-- Star Favorite Box Button -->
    <button
      class="action-box-btn star-box"
      class:is-fav={track.is_favorite}
      onclick={onFavoriteToggle}
      title={track.is_favorite ? 'Unfavorite' : 'Favorite'}
    >
      <Star
        size={16}
        fill={track.is_favorite ? 'var(--accent-lime)' : 'none'}
        color={track.is_favorite ? 'var(--accent-lime)' : 'rgba(255, 255, 255, 0.75)'}
        strokeWidth={1.8}
      />
    </button>

    <!-- Delete Trash Box Button -->
    <button
      class="action-box-btn delete-box"
      onclick={onDelete}
      title="Delete Track"
    >
      <Trash2 size={16} color="rgba(255, 255, 255, 0.75)" strokeWidth={1.8} />
    </button>
  </div>
</div>

<style>
  .track-row {
    width: 100%;
    height: 58px;
    background: #111114;
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 14px;
    display: flex;
    align-items: center;
    padding: 0 18px;
    gap: 16px;
    cursor: pointer;
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
    position: relative;
  }

  .track-row:hover {
    background: #16161B;
    border-color: rgba(255, 255, 255, 0.12);
  }

  /* Active Track with Lime Border matching panel-2.png */
  .track-row.is-active-playing {
    border: 1.5px solid var(--accent-lime);
  }

  .track-play-btn {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: #202026;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: all 0.15s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .track-play-btn:hover {
    transform: scale(1.06);
    background: #2C2C34;
  }

  .track-play-btn.btn-active {
    background: var(--accent-lime);
  }

  .track-info {
    display: flex;
    flex-direction: column;
    width: 175px;
    flex-shrink: 0;
    gap: 3px;
  }

  .track-title {
    font-size: 15px;
    font-weight: 600;
    color: #FFFFFF;
    letter-spacing: -0.2px;
  }

  .track-date {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
  }

  .track-waveform-wrapper {
    flex: 1;
    height: 36px;
    position: relative;
    display: flex;
    align-items: center;
    cursor: col-resize;
    user-select: none;
    padding: 0 2px;
  }

  .track-waveform-wrapper.is-scrubbing {
    cursor: grabbing;
  }

  .waveform-bars {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    pointer-events: none;
  }

  .wave-bar {
    flex: 1;
    min-width: 1px;
    border-radius: 9999px;
    transition: background-color 0.08s ease;
  }

  .wave-bar.is-idle {
    background-color: #282832;
  }

  .wave-bar.is-past {
    background-color: var(--accent-lime);
  }

  .wave-bar:not(.is-past):not(.is-idle) {
    background-color: #383844;
  }

  .playhead-needle {
    position: absolute;
    top: 3px;
    bottom: 3px;
    width: 2px;
    background-color: #FFFFFF;
    pointer-events: none;
    z-index: 10;
    transform: translateX(-50%);
  }

  .playhead-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background-color: #FFFFFF;
    position: absolute;
    top: -2.5px;
    left: 50%;
    transform: translateX(-50%);
    transition: transform 0.12s ease;
  }

  .track-waveform-wrapper:hover .playhead-dot,
  .playhead-needle.is-dragging .playhead-dot {
    transform: translateX(-50%) scale(1.35);
  }

  .track-duration {
    font-size: 12.5px;
    color: rgba(255, 255, 255, 0.55);
    width: 100px;
    text-align: right;
    flex-shrink: 0;
  }

  .track-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .action-box-btn {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: #16161B;
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-box-btn:hover {
    background: #202028;
    border-color: rgba(255, 255, 255, 0.18);
    transform: scale(1.05);
  }

  .delete-box:hover :global(svg) {
    color: #FF453A !important;
  }
</style>
