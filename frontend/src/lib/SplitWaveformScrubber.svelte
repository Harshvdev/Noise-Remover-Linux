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
  export let onSeekStart: ((sec: number) => void) | undefined = undefined;
  export let onSeekMove: ((sec: number) => void) | undefined = undefined;
  export let onSeekEnd: ((sec: number) => void) | undefined = undefined;
  export let onSeek: (sec: number) => void;

  let canvas: HTMLCanvasElement;
  let animId: number;
  let container: HTMLDivElement;
  let scrubberTrackEl: HTMLDivElement;
  let resizeObserver: ResizeObserver | null = null;

  // Floating play/pause button auto-hide state
  let showControls = true;
  let hideTimeout: ReturnType<typeof setTimeout> | null = null;
  export let isDragging = false;
  let dragPosSecs = 0;
  let activePointerCleanup: (() => void) | null = null;

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

  $: activePosSecs = isDragging ? dragPosSecs : currentPosSecs;
  $: playheadRatio = durationSecs > 0 ? Math.min(1, Math.max(0, activePosSecs / durationSecs)) : 0;

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
    if (isPlaying && !isDragging) {
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

      // No glow halo on waveform bars
      ctx.shadowBlur = 0;

      // Interpolate bar color smoothly between Neon Lime #C6FF3D (198, 255, 61, 1.0) and Raw Silver (215, 215, 230, 0.75)
      const r = Math.round(198 * t + 215 * (1 - t));
      const g = Math.round(255 * t + 215 * (1 - t));
      const b = Math.round(61 * t + 230 * (1 - t));
      const a = (1.0 * t + 0.75 * (1 - t)).toFixed(3);
      ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${a})`;

      for (let i = 0; i < barCount; i++) {
        const x = i * barSpacing + barSpacing / 2;
        const sampleProgress = (i + 0.5) / barCount;
        const normIdxClean = Math.min(activeClean.length - 1, Math.floor(sampleProgress * activeClean.length));
        const normIdxRaw = Math.min(activeRaw.length - 1, Math.floor(sampleProgress * activeRaw.length));
        const peakClean = activeClean[normIdxClean] || 0.28;
        const peakRaw = activeRaw[normIdxRaw] || peakClean;
        const peak = peakClean * t + peakRaw * (1 - t);
        const h = Math.max(6, peak * (height * 0.68));

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
    if (activePointerCleanup) {
      activePointerCleanup();
      activePointerCleanup = null;
    }
    if (animId) cancelAnimationFrame(animId);
    if (hideTimeout) clearTimeout(hideTimeout);
    if (resizeObserver) resizeObserver.disconnect();
  });

  function getRatioByClientX(clientX: number, isTimeline = false): number {
    const el = isTimeline ? scrubberTrackEl : container;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return 0;
    const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
    return x / rect.width;
  }

  function startDrag(
    clientX: number,
    isTimeline = false,
    pointerId?: number,
    targetEl?: HTMLElement,
    isDirectPinGrab = false
  ) {
    if (durationSecs <= 0) return;

    if (activePointerCleanup) {
      activePointerCleanup();
      activePointerCleanup = null;
    }

    isDragging = true;
    scheduleHideControls();

    // When directly clicking the needle, maintain its current position rather than jumping
    if (isDirectPinGrab) {
      dragPosSecs = currentPosSecs;
    } else {
      const ratio = getRatioByClientX(clientX, isTimeline);
      dragPosSecs = ratio * durationSecs;
    }

    if (onSeekStart) {
      onSeekStart(dragPosSecs);
    } else {
      onSeek(dragPosSecs);
    }

    const onPointerMove = (ev: PointerEvent) => {
      if (!isDragging) return;
      if (pointerId !== undefined && ev.pointerId !== pointerId) return;
      ev.preventDefault();

      const moveRatio = getRatioByClientX(ev.clientX, isTimeline);
      dragPosSecs = moveRatio * durationSecs;

      if (onSeekMove) {
        onSeekMove(dragPosSecs);
      } else {
        onSeek(dragPosSecs);
      }
    };

    const cleanup = () => {
      window.removeEventListener('pointermove', onPointerMove, true);
      window.removeEventListener('pointerup', onPointerUp, true);
      window.removeEventListener('pointercancel', onPointerCancel, true);
      window.removeEventListener('mousemove', onMouseMoveLegacy, true);
      window.removeEventListener('mouseup', onMouseUpLegacy, true);

      if (targetEl && pointerId !== undefined) {
        try {
          if (targetEl.hasPointerCapture(pointerId)) {
            targetEl.releasePointerCapture(pointerId);
          }
        } catch (_) {}
      }
      activePointerCleanup = null;
    };

    const onPointerUp = (ev: PointerEvent) => {
      if (!isDragging) return;
      if (pointerId !== undefined && ev.pointerId !== pointerId) return;
      ev.preventDefault();
      cleanup();

      isDragging = false;
      const endRatio = getRatioByClientX(ev.clientX, isTimeline);
      const finalSec = endRatio * durationSecs;
      dragPosSecs = finalSec;

      if (onSeekEnd) {
        onSeekEnd(finalSec);
      } else {
        onSeek(finalSec);
      }
    };

    const onPointerCancel = (ev: PointerEvent) => {
      if (!isDragging) return;
      if (pointerId !== undefined && ev.pointerId !== pointerId) return;
      cleanup();

      isDragging = false;
      if (onSeekEnd) {
        onSeekEnd(dragPosSecs);
      } else {
        onSeek(dragPosSecs);
      }
    };

    const onMouseMoveLegacy = (ev: MouseEvent) => {
      if (!isDragging) return;
      ev.preventDefault();
      const moveRatio = getRatioByClientX(ev.clientX, isTimeline);
      dragPosSecs = moveRatio * durationSecs;

      if (onSeekMove) {
        onSeekMove(dragPosSecs);
      } else {
        onSeek(dragPosSecs);
      }
    };

    const onMouseUpLegacy = (ev: MouseEvent) => {
      if (!isDragging) return;
      cleanup();

      isDragging = false;
      const endRatio = getRatioByClientX(ev.clientX, isTimeline);
      const finalSec = endRatio * durationSecs;
      dragPosSecs = finalSec;

      if (onSeekEnd) {
        onSeekEnd(finalSec);
      } else {
        onSeek(finalSec);
      }
    };

    activePointerCleanup = cleanup;

    window.addEventListener('pointermove', onPointerMove, true);
    window.addEventListener('pointerup', onPointerUp, true);
    window.addEventListener('pointercancel', onPointerCancel, true);
    window.addEventListener('mousemove', onMouseMoveLegacy, true);
    window.addEventListener('mouseup', onMouseUpLegacy, true);
  }

  function handleWaveformPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest('.center-play-btn')) return;
    e.preventDefault();

    const target = e.currentTarget as HTMLElement;
    try {
      target.setPointerCapture(e.pointerId);
    } catch (_) {}

    startDrag(e.clientX, false, e.pointerId, target, false);
  }

  function handlePinPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();

    const target = e.currentTarget as HTMLElement;
    try {
      target.setPointerCapture(e.pointerId);
    } catch (_) {}

    startDrag(e.clientX, false, e.pointerId, target, true);
  }

  function handleTimelinePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();

    const target = e.currentTarget as HTMLElement;
    try {
      target.setPointerCapture(e.pointerId);
    } catch (_) {}

    startDrag(e.clientX, true, e.pointerId, target, false);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (isDragging) return;
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
  <!-- Waveform Container (Matches 64px inset of bottom scrubber track for 1:1 timeline alignment) -->
  <div class="waveform-container">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="waveform-stage"
      bind:this={container}
      onpointerdown={handleWaveformPointerDown}
      onmousemove={handleStageMouseMove}
      onmouseleave={handleStageMouseLeave}
      onmousedown={(e) => e.preventDefault()}
      draggable="false"
    >
      <canvas bind:this={canvas}></canvas>

      <!-- Synchronized Playhead Pin contained strictly inside the lime waveform bars with high-visibility white contrast -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="playhead-pin"
        class:is-dragging={isDragging}
        style="left: {playheadRatio * 100}%;"
        onpointerdown={handlePinPointerDown}
        onmousedown={(e) => e.preventDefault()}
        draggable="false"
      >
        <div class="pin-line"></div>
      </div>

      <!-- Centered Play/Pause Button (auto-fades when playing, reappears on hover) -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <button
        class="center-play-btn"
        class:visible={showControls}
        class:is-scrubbing={isDragging}
        onpointerdown={(e) => e.stopPropagation()}
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
  </div>

  <!-- Bottom Timeline Scrubber Row -->
  <div class="timeline-row">
    <span class="time-label time-current tabular-nums">{formatTime(activePosSecs)}</span>

    <!-- Interactive Scrubber Bar -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="scrubber-track"
      bind:this={scrubberTrackEl}
      onpointerdown={handleTimelinePointerDown}
      onmousedown={(e) => e.preventDefault()}
      draggable="false"
      role="slider"
      aria-valuenow={activePosSecs}
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
        class:is-active-drag={isDragging}
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
    gap: clamp(60px, 9vh, 96px);
    align-items: center;
    box-sizing: border-box;
  }

  /* Waveform container with exact 64px padding to align with scrubber track between time labels */
  .waveform-container {
    width: 100%;
    padding: 0 64px;
    box-sizing: border-box;
    display: flex;
    justify-content: center;
  }

  .waveform-stage {
    width: 100%;
    height: clamp(170px, 26vh, 250px);
    position: relative;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
    touch-action: none;
  }

  canvas {
    width: 100%;
    height: 100%;
    display: block;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
  }

  /* Synchronized vertical needle pin contained inside waveform bars */
  .playhead-pin {
    position: absolute;
    top: 16%;
    bottom: 16%;
    width: 2.5px;
    pointer-events: auto;
    cursor: grab;
    transform: translateX(-50%);
    z-index: 25;
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
    touch-action: none;
  }

  /* Generous invisible grab hit-area so users can easily click and drag the pin directly */
  .playhead-pin::before {
    content: '';
    position: absolute;
    top: -12px;
    bottom: -12px;
    left: -16px;
    right: -16px;
    cursor: grab;
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
    touch-action: none;
  }

  .playhead-pin.is-dragging,
  .playhead-pin.is-dragging::before {
    cursor: grabbing;
  }

  /* Distinct high-contrast needle with crisp outline without glow */
  .playhead-pin .pin-line {
    width: 100%;
    height: 100%;
    background-color: #FFFFFF;
    box-shadow: 0 0 0 1.5px rgba(0, 0, 0, 0.85);
    border-radius: 9999px;
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
    user-select: none;
    -webkit-user-select: none;
    transition: opacity 0.24s cubic-bezier(0.16, 1, 0.3, 1),
      transform 0.24s cubic-bezier(0.16, 1, 0.3, 1),
      background-color 0.15s ease;
  }

  .center-play-btn.visible {
    opacity: 1;
    pointer-events: auto;
    transform: translate(-50%, -50%) scale(1);
  }

  .center-play-btn.is-scrubbing {
    pointer-events: none !important;
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
    user-select: none;
    -webkit-user-select: none;
  }

  .time-label {
    font-size: 13.5px;
    color: #FFFFFF;
    font-weight: 600;
    width: 48px;
    flex-shrink: 0;
    user-select: none;
    -webkit-user-select: none;
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
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
    touch-action: none;
  }

  .scrubber-fill {
    height: 100%;
    background-color: #FFFFFF;
    border-radius: 9999px;
    transition: background-color 0.3s ease;
  }

  .scrubber-fill.clean-fill {
    background-color: var(--accent-lime);
  }

  .scrubber-thumb {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background-color: #FFFFFF;
    transform: translate(-50%, -50%);
    transition: transform 0.15s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.3s ease;
  }

  .scrubber-thumb.clean-thumb {
    background-color: var(--accent-lime);
  }

  .scrubber-track:hover .scrubber-thumb,
  .scrubber-thumb.is-active-drag {
    transform: translate(-50%, -50%) scale(1.25);
  }

  @media (max-height: 720px) {
    .split-scrubber-widget {
      gap: clamp(48px, 7.5vh, 68px);
    }
    .waveform-stage {
      height: clamp(130px, 20vh, 160px);
    }
  }

  @media (max-height: 560px) {
    .split-scrubber-widget {
      gap: 36px;
    }
    .waveform-stage {
      height: 110px;
    }
  }

  @media (max-width: 480px) {
    .waveform-container {
      padding: 0 52px;
    }
    .timeline-row {
      gap: 10px;
    }
    .time-label {
      width: 42px;
      font-size: 12px;
    }
  }
</style>
