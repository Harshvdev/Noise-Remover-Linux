<script lang="ts">
  import { Mic, Square } from '@lucide/svelte';

  export let isRecording = false;
  export let isCalibrating = false;
  export let onToggle: () => void;
</script>

<div class="record-wrapper">
  <!-- Outer Dark Bezel Container -->
  <div class="outer-bezel">
    <!-- Lime Green Fixed Arc SVG -->
    <svg class="meter-svg" viewBox="0 0 144 144">
      <!-- Complete dark circular track groove (bottom portion shows when neon arc ends) -->
      <circle
        cx="72"
        cy="72"
        r="58"
        class="track-circle"
      />

      <!-- Sharp, Clean Fixed Neon Lime Arc (spanning 214° symmetrically across 12 o'clock, no glow) -->
      <path
        d="M 16.53 88.96 A 58 58 0 1 1 127.47 88.96"
        class="neon-arc"
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
    width: 144px;
    height: 144px;
    border-radius: 50%;
    background: var(--bezel-bg);
    border: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    box-shadow: var(--shadow-card);
  }

  .meter-svg {
    position: absolute;
    top: 0;
    left: 0;
    width: 144px;
    height: 144px;
    pointer-events: none;
  }

  .track-circle {
    fill: none;
    stroke: var(--bezel-track);
    stroke-width: 4.5;
  }

  .neon-arc {
    fill: none;
    stroke: var(--accent-lime);
    stroke-width: 4.5;
    stroke-linecap: butt;
  }

  .record-btn {
    width: 94px;
    height: 94px;
    border-radius: 50%;
    background-color: #ED2B2F;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    transition: transform 0.18s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.15s ease;
    z-index: 2;
  }

  .record-btn:hover {
    transform: scale(1.04);
    background-color: #F8363A;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.45);
  }

  .record-btn:active {
    transform: scale(0.96);
  }

  .record-btn.is-recording {
    border-radius: 50%;
    animation: recording-pulse 1.8s infinite ease-in-out;
  }

  @keyframes recording-pulse {
    0%, 100% {
      transform: scale(1);
    }
    50% {
      transform: scale(1.04);
    }
  }

  .caption {
    font-size: 14px;
    color: var(--text-muted);
    font-weight: 400;
    letter-spacing: -0.2px;
  }
</style>
