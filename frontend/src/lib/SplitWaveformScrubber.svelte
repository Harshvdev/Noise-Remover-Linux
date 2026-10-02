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
  let scrubberTrackEl: HTMLDivElement;
  let resizeObserver: ResizeObserver | null = null;

  // Floating play/pause button auto-hide state
  let showControls = true;
  let hideTimeout: ReturnType<typeof setTimeout> | null = null;
  let isDraggingScrubber = false;
  let isDraggingWaveform = false;

  // Smooth enhance A/B cross-fade transition state
  let enhanceProgress = isCleanAudio ? 1.0 : 0.0;
  let lastRenderTime = performance.now();

  // Static grain particles for Raw mode
  interface RawGrain {
    x: number;
    y: number;
    size: number;
    alpha: number;
  }
  let rawGrains: RawGrain[] = [];

  function initRawGrains(width: number, height: number) {
    rawGrains = [];
    const count = 1200;
    for (let i = 0; i < count; i++) {
      // Gaussian-clustered y offset around center with organic spread
      const u1 = Math.max(1e-4, Math.random());
      const u2 = Math.random();
      const randNorm = Math.sqrt(-2.0 * Math.log(u1)) * Math.cos(2.0 * Math.PI * u2);
      const ySpread = randNorm * (height * 0.28);

      rawGrains.push({
        x: Math.random() * width,
        y: ySpread,
        size: Math.random() * 1.5 + 0.5,
        alpha: Math.random() * 0.45 + 0.12,
      });
    }
  }

  function formatTime(s: number): string {
    const safe = Math.max(0, s);
    const mins = Math.floor(safe / 60);
    const secs = Math.floor(safe % 60);
    return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }

  $: playheadRatio = durationSecs > 0 ? Math.min(1, Math.max(0, currentPosSecs / durationSecs)) : 0;

  function scheduleHideControls() {
    if (hideTimeout) clearTimeout(hideTimeout);
    showControls = true;
    if (isPlaying) {
      hideTimeout = setTimeout(() => {
        showControls = false;
      }, 2000);
    }
  }

  function handleStageMouseMove() {
    scheduleHideControls();
  }

  function handleStageMouseLeave() {
    if (isPlaying && !isDraggingWaveform) {
      showControls = false;
    }
  }

  $: if (!isPlaying) {
    showControls = true;
    if (hideTimeout) clearTimeout(hideTimeout);
  } else {
    scheduleHideControls();
  }

  onMount(() => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const resize = () => {
      if (!canvas) return;
      const rect = canvas.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.scale(dpr, dpr);
      initRawGrains(rect.width, rect.height);
    };

    resize();
    window.addEventListener('resize', resize);

    if (typeof ResizeObserver !== 'undefined' && container) {
      resizeObserver = new ResizeObserver(() => {
        resize();
      });
      resizeObserver.observe(container);
    }

    const render = () => {
      const rect = canvas.getBoundingClientRect();
      const width = rect.width;
      const height = rect.height;
      if (width <= 0 || height <= 0) {
        animId = requestAnimationFrame(render);
        return;
      }
      const centerY = height / 2;

      // Smooth delta-time transition for enhance toggle (fade in/out over ~300ms)
      const now = performance.now();
      const dt = Math.min(0.08, (now - lastRenderTime) / 1000);
      lastRenderTime = now;

      const target = isCleanAudio ? 1.0 : 0.0;
      enhanceProgress += (target - enhanceProgress) * Math.min(1.0, dt * 10.0);
      if (Math.abs(target - enhanceProgress) < 0.002) {
        enhanceProgress = target;
      }
      const t = enhanceProgress;

      ctx.clearRect(0, 0, width, height);

      // 1. Noise hiss particles (fade in when raw, fade out when clean)
      if (t < 0.99) {
        const mistAlpha = 1.0 - t;
        ctx.shadowBlur = 0;
        for (const g of rawGrains) {
          ctx.fillStyle = `rgba(220, 220, 235, ${(g.alpha * mistAlpha).toFixed(3)})`;
          ctx.beginPath();
          ctx.arc(g.x, centerY + g.y, g.size, 0, Math.PI * 2);
          ctx.fill();
        }
      }

      // 2. Waveform bars with smooth cross-fade between Clean (neon lime) and Raw (silver)
      const barCount = Math.max(64, Math.min(180, Math.round(width / 7.5)));
      const barSpacing = width / barCount;
      const barWidth = Math.max(2.4, barSpacing * 0.56);

      const activeClean = cleanWaveform.length > 0 ? cleanWaveform : rawWaveform;
      const activeRaw = rawWaveform.length > 0 ? rawWaveform : cleanWaveform;

      // Glow halo: blooms when clean (t=1), dims when raw (t=0)
      if (t > 0.02) {
        ctx.shadowColor = '#C6FF3D';
        ctx.shadowBlur = Math.round(10 * t);
      } else {
        ctx.shadowBlur = 0;
      }

      // Interpolate bar color smoothly between Neon Lime #C6FF3D (198, 255, 61, 1.0) and Raw Silver (215, 215, 230, 0.75)
      const r = Math.round(198 * t + 215 * (1 - t));
      const g = Math.round(255 * t + 215 * (1 - t));
      const b = Math.round(61 * t + 230 * (1 - t));
      const a = (1.0 * t + 0.75 * (1 - t)).toFixed(3);
      ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${a})`;

      for (let i = 0; i < barCount; i++) {
        const x = i * barSpacing + barSpacing / 2;
        const normIdxClean = Math.floor((i / barCount) * activeClean.length);
        const normIdxRaw = Math.floor((i / barCount) * activeRaw.length);
        const peakClean = activeClean[normIdxClean] || 0.28;
        const peakRaw = activeRaw[normIdxRaw] || peakClean;
        const peak = peakClean * t + peakRaw * (1 - t);
        const h = Math.max(6, peak * (height * 0.78));

        ctx.beginPath();
        ctx.roundRect(x - barWidth / 2, centerY - h / 2, barWidth, h, 9999);
        ctx.fill();
      }

      animId = requestAnimationFrame(render);
    };

    animId = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', resize);
      if (resizeObserver) {
        resizeObserver.disconnect();
      }
    };
  });

  onDestroy(() => {
    if (animId) cancelAnimationFrame(animId);
    if (hideTimeout) clearTimeout(hideTimeout);
    if (resizeObserver) resizeObserver.disconnect();
  });

  // Direct waveform seek & drag
  function seekWaveformByClientX(clientX: number) {
    if (!container || durationSecs <= 0) return;
    const rect = container.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
    const ratio = rect.width > 0 ? x / rect.width : 0;
    onSeek(ratio * durationSecs);
  }

  function handleWaveformMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    // If clicked directly on the play button, don't seek
    if ((e.target as HTMLElement).closest('.center-play-btn')) return;

    isDraggingWaveform = true;
    scheduleHideControls();
    seekWaveformByClientX(e.clientX);

    const onMouseMove = (ev: MouseEvent) => {
      if (!isDraggingWaveform) return;
      seekWaveformByClientX(ev.clientX);
    };

    const onMouseUp = () => {
      isDraggingWaveform = false;
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  }

  // Handle timeline scrubber bar click & drag
  function seekTimelineByClientX(clientX: number) {
    if (!scrubberTrackEl || durationSecs <= 0) return;
    const rect = scrubberTrackEl.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
    const ratio = rect.width > 0 ? x / rect.width : 0;
    onSeek(ratio * durationSecs);
  }

  function handleTimelineMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    isDraggingScrubber = true;
    scheduleHideControls();
    seekTimelineByClientX(e.clientX);

    const onMouseMove = (moveEv: MouseEvent) => {
      if (!isDraggingScrubber) return;
      seekTimelineByClientX(moveEv.clientX);
    };

    const onMouseUp = () => {
      isDraggingScrubber = false;
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === ' ' || e.code === 'Space') {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement
      ) {
        return;
      }
      e.preventDefault();
      onPlayToggle();
      scheduleHideControls();
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<div class="split-scrubber-widget">
  <!-- Waveform Stage (Interactive Canvas + Synchronized Pin + Centered Hover Play Button) -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="waveform-stage"
    bind:this={container}
    onmousedown={handleWaveformMouseDown}
    onmousemove={handleStageMouseMove}
    onmouseleave={handleStageMouseLeave}
  >
    <canvas bind:this={canvas}></canvas>

    <!-- Synchronized Playhead Pin that moves with playback & bottom audio bar -->
    <div
      class="playhead-pin"
      class:clean-pin={isCleanAudio}
      style="left: {playheadRatio * 100}%;"
    >
      <div class="pin-line"></div>
    </div>

    <!-- Centered Play/Pause Button (auto-fades when playing, reappears on hover) -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <button
      class="center-play-btn"
      class:visible={showControls}
      onclick={(e) => {
        e.stopPropagation();
        onPlayToggle();
        scheduleHideControls();
      }}
      title={isPlaying ? 'Pause (Space)' : 'Play (Space)'}
      aria-label={isPlaying ? 'Pause' : 'Play'}
    >
      {#if isPlaying}
        <Pause size={22} fill="#000000" color="#000000" />
      {:else}
        <Play size={22} fill="#000000" color="#000000" style="margin-left: 2px;" />
      {/if}
    </button>
  </div>

  <!-- Bottom Timeline Scrubber Row -->
  <div class="timeline-row">
    <span class="time-label time-current tabular-nums">{formatTime(currentPosSecs)}</span>

    <!-- Interactive Scrubber Bar -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="scrubber-track"
      bind:this={scrubberTrackEl}
      onmousedown={handleTimelineMouseDown}
      role="slider"
      aria-valuenow={currentPosSecs}
      aria-valuemin={0}
      aria-valuemax={durationSecs}
      tabindex="0"
    >
      <div
        class="scrubber-fill"
        class:clean-fill={isCleanAudio}
        style="width: {playheadRatio * 100}%;"
      ></div>
      <div
        class="scrubber-thumb"
        class:clean-thumb={isCleanAudio}
        class:is-active-drag={isDraggingScrubber}
        style="left: {playheadRatio * 100}%;"
      ></div>
    </div>

    <span class="time-label time-total tabular-nums">{formatTime(durationSecs)}</span>
  </div>
</div>

<style>
  .split-scrubber-widget {
    width: 100%;
    max-width: 1000px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    align-items: center;
    box-sizing: border-box;
  }

  .waveform-stage {
    width: 100%;
    height: clamp(190px, 30vh, 290px);
    position: relative;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
  }

  canvas {
    width: 100%;
    height: 100%;
    display: block;
    pointer-events: none;
  }

  /* Synchronized vertical needle pin */
  .playhead-pin {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    pointer-events: none;
    transform: translateX(-50%);
    z-index: 8;
  }

  .playhead-pin .pin-line {
    width: 100%;
    height: 100%;
    background-color: #FFFFFF;
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.5);
    border-radius: 9999px;
    transition: background-color 0.3s ease, box-shadow 0.3s ease;
  }

  .playhead-pin.clean-pin .pin-line {
    background-color: var(--accent-lime);
    box-shadow: 0 0 10px var(--accent-lime-glow);
  }

  /* Centered floating play/pause button */
  .center-play-btn {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%) scale(0.92);
    width: 54px;
    height: 54px;
    border-radius: 50%;
    background-color: #FFFFFF;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.65);
    cursor: pointer;
    z-index: 15;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.24s cubic-bezier(0.16, 1, 0.3, 1),
      transform 0.24s cubic-bezier(0.16, 1, 0.3, 1),
      background-color 0.15s ease;
  }

  .center-play-btn.visible {
    opacity: 1;
    pointer-events: auto;
    transform: translate(-50%, -50%) scale(1);
  }

  .center-play-btn:hover {
    transform: translate(-50%, -50%) scale(1.08);
    background-color: #F8F8F8;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.75);
  }

  .center-play-btn:active {
    transform: translate(-50%, -50%) scale(0.95);
  }

  /* Bottom Timeline Scrubber Row */
  .timeline-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 16px;
    box-sizing: border-box;
  }

  .time-label {
    font-size: 13.5px;
    color: #FFFFFF;
    font-weight: 600;
    width: 48px;
    flex-shrink: 0;
  }

  .time-current {
    text-align: left;
  }

  .time-total {
    text-align: right;
  }

  .scrubber-track {
    flex: 1;
    height: 4px;
    background: #202026;
    border-radius: 9999px;
    position: relative;
    cursor: pointer;
    display: flex;
    align-items: center;
  }

  .scrubber-fill {
    height: 100%;
    background-color: #FFFFFF;
    border-radius: 9999px;
    box-shadow: 0 0 6px rgba(255, 255, 255, 0.3);
    transition: background-color 0.3s ease, box-shadow 0.3s ease;
  }

  .scrubber-fill.clean-fill {
    background-color: var(--accent-lime);
    box-shadow: 0 0 6px var(--accent-lime-glow);
  }

  .scrubber-thumb {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background-color: #FFFFFF;
    transform: translate(-50%, -50%);
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.6);
    transition: transform 0.15s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.3s ease, box-shadow 0.3s ease;
  }

  .scrubber-thumb.clean-thumb {
    background-color: var(--accent-lime);
    box-shadow: 0 0 8px rgba(198, 255, 61, 0.7);
  }

  .scrubber-track:hover .scrubber-thumb,
  .scrubber-thumb.is-active-drag {
    transform: translate(-50%, -50%) scale(1.25);
  }

  @media (max-height: 720px) {
    .split-scrubber-widget {
      gap: 12px;
    }
    .waveform-stage {
      height: clamp(150px, 25vh, 200px);
    }
  }
</style>
