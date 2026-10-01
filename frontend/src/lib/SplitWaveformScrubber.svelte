<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Play, Pause } from '@lucide/svelte';

  export let cleanWaveform: number[] = [];
  export let rawWaveform: number[] = [];
  export let durationSecs = 134;
  export let currentPosSecs = 34;
  export let isPlaying = false;
  export let isCleanAudio = true;
  export let onPlayToggle: () => void;
  export let onSeek: (sec: number) => void;

  let canvas: HTMLCanvasElement;
  let animId: number;
  let container: HTMLDivElement;

  // Split reveal divider position (0.0 to 1.0)
  let wipeRatio = 0.48;
  let isDraggingWipe = false;

  // Static grain particles for Raw side
  interface RawGrain {
    x: number;
    y: number;
    size: number;
    alpha: number;
  }
  let rawGrains: RawGrain[] = [];

  function initRawGrains(width: number, height: number) {
    rawGrains = [];
    const count = 400;
    for (let i = 0; i < count; i++) {
      rawGrains.push({
        x: Math.random() * width,
        y: (Math.random() - 0.5) * height * 0.75,
        size: Math.random() * 1.6 + 0.6,
        alpha: Math.random() * 0.45 + 0.15,
      });
    }
  }

  function formatTime(s: number): string {
    const mins = Math.floor(s / 60);
    const secs = Math.floor(s % 60);
    return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }

  onMount(() => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.scale(dpr, dpr);
      initRawGrains(rect.width, rect.height);
    };

    resize();
    window.addEventListener('resize', resize);

    const render = () => {
      const rect = canvas.getBoundingClientRect();
      const width = rect.width;
      const height = rect.height;
      const centerY = height / 2;
      const wipeX = width * wipeRatio;

      ctx.clearRect(0, 0, width, height);

      const barCount = 100;
      const barSpacing = width / barCount;
      const barWidth = 3.2;

      // 1. Draw Clean Side (Left of wipeX)
      ctx.save();
      ctx.beginPath();
      ctx.rect(0, 0, wipeX, height);
      ctx.clip();

      for (let i = 0; i < barCount; i++) {
        const x = i * barSpacing + barSpacing / 2;
        if (x > wipeX + 2) break;

        const normIdx = Math.floor((i / barCount) * cleanWaveform.length);
        const peak = cleanWaveform[normIdx] || 0.3;
        const h = Math.max(6, peak * (height * 0.76));

        ctx.shadowColor = '#C6FF3D';
        ctx.shadowBlur = 12;
        ctx.fillStyle = '#C6FF3D';

        ctx.beginPath();
        ctx.roundRect(x - barWidth / 2, centerY - h / 2, barWidth, h, 2);
        ctx.fill();
      }
      ctx.restore();

      // 2. Draw Raw Side (Right of wipeX) - stippled grey static grain
      ctx.save();
      ctx.beginPath();
      ctx.rect(wipeX, 0, width - wipeX, height);
      ctx.clip();

      // Draw grain particles
      for (const g of rawGrains) {
        if (g.x >= wipeX) {
          ctx.fillStyle = `rgba(200, 200, 215, ${g.alpha})`;
          ctx.beginPath();
          ctx.arc(g.x, centerY + g.y, g.size, 0, Math.PI * 2);
          ctx.fill();
        }
      }

      // Draw raw waveform bars as stippled/fuzzy grey
      for (let i = 0; i < barCount; i++) {
        const x = i * barSpacing + barSpacing / 2;
        if (x < wipeX - 2) continue;

        const normIdx = Math.floor((i / barCount) * rawWaveform.length);
        const peak = rawWaveform[normIdx] || 0.3;
        const h = Math.max(6, peak * (height * 0.76));

        ctx.shadowBlur = 0;
        ctx.fillStyle = 'rgba(210, 210, 225, 0.75)';

        ctx.beginPath();
        ctx.roundRect(x - barWidth / 2, centerY - h / 2, barWidth, h, 2);
        ctx.fill();
      }
      ctx.restore();

      animId = requestAnimationFrame(render);
    };

    animId = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', resize);
    };
  });

  onDestroy(() => {
    if (animId) cancelAnimationFrame(animId);
  });

  // Handle dragging the vertical wipe divider
  function startWipeDrag(e: MouseEvent) {
    isDraggingWipe = true;
    handleWipeMove(e);
    window.addEventListener('mousemove', handleWipeMove);
    window.addEventListener('mouseup', stopWipeDrag);
  }

  function handleWipeMove(e: MouseEvent) {
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const x = e.clientX - rect.left;
    wipeRatio = Math.max(0.05, Math.min(0.95, x / rect.width));
  }

  function stopWipeDrag() {
    isDraggingWipe = false;
    window.removeEventListener('mousemove', handleWipeMove);
    window.removeEventListener('mouseup', stopWipeDrag);
  }

  // Handle timeline scrubber drag
  function handleTimelineClick(e: MouseEvent) {
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    onSeek(ratio * durationSecs);
  }
</script>

<div class="split-scrubber-widget">
  <!-- Center Comparison Waveform Hero -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="waveform-stage"
    bind:this={container}
    onmousedown={startWipeDrag}
  >
    <canvas bind:this={canvas}></canvas>

    <!-- Clean / Raw Section Labels -->
    <div
      class="label-clean"
      class:label-active={isCleanAudio}
      style="left: {Math.max(10, wipeRatio * 100 - 6)}%;"
    >
      Clean
    </div>
    <div
      class="label-raw"
      class:label-active={!isCleanAudio}
      style="left: {Math.min(94, wipeRatio * 100 + 3)}%;"
    >
      Raw
    </div>

    <!-- Draggable Vertical Wipe Divider Line -->
    <div
      class="wipe-divider"
      style="left: {wipeRatio * 100}%;"
    >
      <!-- Center Circular Play Button -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <button
        class="center-play-btn"
        onclick={(e) => {
          e.stopPropagation();
          onPlayToggle();
        }}
        title={isPlaying ? 'Pause' : 'Play'}
      >
        {#if isPlaying}
          <Pause size={22} fill="#0B0B0D" color="#0B0B0D" />
        {:else}
          <Play size={22} fill="#0B0B0D" color="#0B0B0D" style="margin-left: 3px;" />
        {/if}
      </button>
    </div>
  </div>

  <!-- Bottom Timeline Scrubber Row -->
  <div class="timeline-row">
    <span class="time-current tabular-nums">{formatTime(currentPosSecs)}</span>

    <!-- Scrubber Bar -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="scrubber-track"
      onclick={handleTimelineClick}
      role="slider"
      aria-valuenow={currentPosSecs}
      aria-valuemin={0}
      aria-valuemax={durationSecs}
      tabindex="0"
    >
      <div
        class="scrubber-fill"
        style="width: {durationSecs > 0 ? (currentPosSecs / durationSecs) * 100 : 0}%;"
      ></div>
      <div
        class="scrubber-thumb"
        style="left: {durationSecs > 0 ? (currentPosSecs / durationSecs) * 100 : 0}%;"
      ></div>
    </div>

    <span class="time-total tabular-nums">{formatTime(durationSecs)}</span>
  </div>
</div>

<style>
  .split-scrubber-widget {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 20px;
    align-items: center;
  }

  .waveform-stage {
    width: 100%;
    height: 240px;
    position: relative;
    cursor: ew-resize;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  canvas {
    width: 100%;
    height: 100%;
    display: block;
    pointer-events: none;
  }

  .label-clean {
    position: absolute;
    top: 14px;
    transform: translateX(-100%);
    font-size: 13.5px;
    font-weight: 600;
    color: var(--accent-lime);
    letter-spacing: -0.2px;
    pointer-events: none;
    text-shadow: 0 0 10px var(--accent-lime-glow);
  }

  .label-raw {
    position: absolute;
    top: 14px;
    font-size: 13.5px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.75);
    letter-spacing: -0.2px;
    pointer-events: none;
  }

  .wipe-divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background-color: var(--accent-lime);
    box-shadow: 0 0 10px var(--accent-lime-glow);
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    transform: translateX(-50%);
    z-index: 10;
  }

  .center-play-btn {
    pointer-events: auto;
    width: 58px;
    height: 58px;
    min-width: 58px;
    min-height: 58px;
    flex-shrink: 0;
    border-radius: 50%;
    background-color: #FFFFFF;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
    cursor: pointer;
    transition: transform 0.15s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.15s ease;
  }

  .center-play-btn:hover {
    transform: scale(1.1);
    background-color: #F0F0F0;
  }

  .center-play-btn:active {
    transform: scale(0.96);
  }

  .timeline-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .time-current, .time-total {
    font-size: 13.5px;
    color: var(--text-muted);
    font-weight: 500;
    width: 48px;
  }

  .time-current {
    text-align: left;
  }

  .time-total {
    text-align: right;
  }

  .scrubber-track {
    flex: 1;
    height: 6px;
    background: #23232A;
    border-radius: 9999px;
    position: relative;
    cursor: pointer;
    display: flex;
    align-items: center;
  }

  .scrubber-fill {
    height: 100%;
    background-color: var(--accent-lime);
    border-radius: 9999px;
    box-shadow: 0 0 8px var(--accent-lime-glow);
  }

  .scrubber-thumb {
    position: absolute;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background-color: var(--accent-lime);
    transform: translate(-50%, 0);
    box-shadow: 0 0 10px var(--accent-lime-glow);
    transition: transform 0.1s ease;
  }

  .scrubber-track:hover .scrubber-thumb {
    transform: translate(-50%, 0) scale(1.2);
  }

  @media (max-height: 720px) {
    .waveform-stage {
      height: 180px;
    }
  }
</style>
