<script lang="ts">
  import { onMount } from 'svelte';

  export let onFinish: () => void;

  let isFadingOut = false;

  onMount(() => {
    // Show splash for 1.4s, then smooth fade out
    const timer = setTimeout(() => {
      isFadingOut = true;
      setTimeout(() => {
        onFinish();
      }, 400);
    }, 1400);

    return () => clearTimeout(timer);
  });
</script>

<div class="splash-screen" class:fade-out={isFadingOut}>
  <div class="splash-content">
    <!-- Animated Logo Waveform Bars -->
    <div class="splash-logo">
      <span class="s-bar s-bar-1"></span>
      <span class="s-bar s-bar-2"></span>
      <span class="s-bar s-bar-3"></span>
      <span class="s-bar s-bar-4"></span>
      <span class="s-bar s-bar-5"></span>
    </div>

    <!-- App Name and Tagline -->
    <div class="splash-text">
      <h1 class="splash-title">Voice Cleaner</h1>
      <p class="splash-subtitle">Clear Voice. Pure Sound.</p>
    </div>
  </div>
</div>

<style>
  .splash-screen {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
    background-color: var(--bg-app);
    z-index: 99999;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: opacity 0.38s cubic-bezier(0.16, 1, 0.3, 1),
                transform 0.38s cubic-bezier(0.16, 1, 0.3, 1);
    pointer-events: auto;
    overflow: hidden;
  }

  .splash-screen.fade-out {
    opacity: 0;
    transform: scale(1.03);
    pointer-events: none;
  }

  .splash-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    animation: splashAppear 0.5s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  @keyframes splashAppear {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .splash-logo {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 52px;
  }

  .s-bar {
    width: 6px;
    background-color: var(--accent-lime);
    border-radius: 9999px;
    animation: barPulse 1.2s infinite ease-in-out;
  }

  .s-bar-1 { height: 22px; animation-delay: 0.0s; }
  .s-bar-2 { height: 40px; animation-delay: 0.15s; }
  .s-bar-3 { height: 52px; animation-delay: 0.3s; }
  .s-bar-4 { height: 34px; animation-delay: 0.45s; }
  .s-bar-5 { height: 18px; animation-delay: 0.6s; }

  @keyframes barPulse {
    0%, 100% {
      transform: scaleY(1);
      opacity: 0.85;
    }
    50% {
      transform: scaleY(1.18);
      opacity: 1;
    }
  }

  .splash-text {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    text-align: center;
  }

  .splash-title {
    font-family: var(--font-brand);
    font-size: 32px;
    font-weight: 700;
    color: var(--text-main);
    letter-spacing: -0.6px;
    line-height: 1.1;
    margin: 0;
  }

  .splash-subtitle {
    font-family: var(--font-body);
    font-size: 14.5px;
    color: var(--text-muted);
    font-weight: 400;
    margin: 0;
    letter-spacing: -0.1px;
  }
</style>
