<script lang="ts">
  import { Mic, Square } from '@lucide/svelte';

  export let isRecording = false;
  export let isCalibrating = false;
  export let level = 0.0; // 0.0 to 1.0
  export let onToggle: () => void;

  // Arc calculation for SVG meter ring
  // Center: (68, 68), Radius: 54 -> Circumference ≈ 339.29
  const CIRCUMFERENCE = 2 * Math.PI * 54;
  
  // Base arc coverage in idle (about 42% of circumference matching panel-1.png)
  // When active/level rises, arc smoothly sweeps up to full circle (1.0)
  $: activeFraction = isCalibrating
    ? 0.55
    : isRecording
      ? Math.min(1.0, 0.5 + level * 0.5)
      : Math.min(1.0, 0.42 + level * 0.58);

  $: dashOffset = CIRCUMFERENCE * (1 - activeFraction);
  $: glowIntensity = isRecording ? 0.6 + level * 0.4 : isCalibrating ? 0.5 : 0.25 + level * 0.4;
</script>

<div class="record-wrapper">
  <!-- Outer Dark Bezel Container -->
  <div class="outer-bezel">
    <!-- Lime Green Level Arc SVG -->
    <svg class="meter-svg" viewBox="0 0 136 136">
      <defs>
        <filter id="lime-glow" x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="3.5" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      <!-- Faint track circle -->
      <circle
        cx="68"
        cy="68"
        r="54"
        class="track-circle"
      />

      <!-- Dynamic Lime-Green Glowing Arc -->
      <circle
        cx="68"
        cy="68"
        r="54"
        class="arc-circle"
        style="
          stroke-dasharray: {CIRCUMFERENCE};
          stroke-dashoffset: {dashOffset};
          opacity: {0.7 + glowIntensity * 0.3};
          filter: drop-shadow(0 0 {6 * glowIntensity}px rgba(198, 255, 61, {glowIntensity}));
        "
      />
    </svg>

    <!-- Inner Red Record Button -->
    <button
      class="record-btn"
      class:is-recording={isRecording}
      onclick={onToggle}
      aria-label={isRecording ? 'Stop Recording' : 'Start Recording'}
    >
      {#if isRecording}
        <Square size={26} fill="#FFFFFF" color="#FFFFFF" />
      {:else}
        <Mic size={36} color="#FFFFFF" strokeWidth={2.4} />
      {/if}
    </button>
  </div>

  <!-- Caption Text -->
  <p class="caption">
    {#if isCalibrating}
      Listening to room noise...
    {:else if isRecording}
      Click to stop recording
    {:else}
      Click to start recording
    {/if}
  </p>
</div>

<style>
  .record-wrapper {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }

  .outer-bezel {
    width: 136px;
    height: 136px;
    border-radius: 50%;
    background: #141417;
    border: 1px solid rgba(255, 255, 255, 0.07);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.45);
  }

  .meter-svg {
    position: absolute;
    top: 0;
    left: 0;
    width: 136px;
    height: 136px;
    pointer-events: none;
    transform: rotate(-135deg); /* Orient arc at top-left across to top-right matching mockup */
  }

  .track-circle {
    fill: none;
    stroke: rgba(255, 255, 255, 0.04);
    stroke-width: 3.5;
  }

  .arc-circle {
    fill: none;
    stroke: var(--accent-lime);
    stroke-width: 3.5;
    stroke-linecap: round;
    transition: stroke-dashoffset 0.08s ease-out, filter 0.15s ease, opacity 0.15s ease;
  }

  .record-btn {
    width: 90px;
    height: 90px;
    border-radius: 50%;
    background-color: #ED2B2F;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    box-shadow: 0 6px 20px rgba(237, 43, 47, 0.42);
    transition: transform 0.18s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.15s ease, border-radius 0.22s ease;
    z-index: 2;
  }

  .record-btn:hover {
    transform: scale(1.04);
    background-color: #F8363A;
    box-shadow: 0 8px 26px rgba(237, 43, 47, 0.6);
  }

  .record-btn:active {
    transform: scale(0.96);
  }

  .record-btn.is-recording {
    border-radius: 24px;
    animation: recording-pulse 1.8s infinite ease-in-out;
  }

  @keyframes recording-pulse {
    0%, 100% {
      box-shadow: 0 0 16px rgba(237, 43, 47, 0.45);
    }
    50% {
      box-shadow: 0 0 30px rgba(237, 43, 47, 0.85);
    }
  }

  .caption {
    font-size: 14px;
    color: rgba(255, 255, 255, 0.45);
    font-weight: 400;
    letter-spacing: -0.2px;
  }
</style>
