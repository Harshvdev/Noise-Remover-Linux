<script lang="ts">
  import { onMount } from 'svelte';

  export let onComplete: () => void;
  export let onCancel: () => void;

  let secondsLeft = 3;
  let label = '3';
  let timer: ReturnType<typeof setInterval>;

  onMount(() => {
    timer = setInterval(() => {
      secondsLeft -= 1;
      if (secondsLeft === 2) {
        label = '2';
      } else if (secondsLeft === 1) {
        label = '1';
      } else if (secondsLeft === 0) {
        label = 'Start!';
      } else {
        clearInterval(timer);
        onComplete();
      }
    }, 1000);

    return () => {
      if (timer) clearInterval(timer);
    };
  });
</script>

<div class="countdown-backdrop">
  <div class="countdown-modal">
    <div class="countdown-ring">
      <span class="count-number" class:is-text={label === 'Start!'}>{label}</span>
    </div>

    <div class="countdown-text">
      <h3>Calibrating Room Noise</h3>
      <p>Please remain quiet. Capturing 3 seconds of background noise to isolate your voice.</p>
    </div>

    <button class="btn-cancel" onclick={onCancel}>Cancel</button>
  </div>
</div>

<style>
  .countdown-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(11, 11, 13, 0.88);
    backdrop-filter: blur(12px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    animation: fadeIn 0.2s ease-out;
  }

  .countdown-modal {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    max-width: 440px;
    padding: 36px 32px;
    background: #16161A;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 24px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
  }

  .countdown-ring {
    width: 110px;
    height: 110px;
    border-radius: 50%;
    border: 3px solid var(--accent-lime);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 24px;
    animation: pulseScale 1s infinite cubic-bezier(0.16, 1, 0.3, 1);
  }

  .count-number {
    font-family: var(--font-brand);
    font-size: 42px;
    font-weight: 700;
    color: var(--accent-lime);
    line-height: 1;
    user-select: none;
    transition: font-size 0.15s ease;
  }

  .count-number.is-text {
    font-size: 24px;
    letter-spacing: -0.2px;
  }

  .countdown-text h3 {
    font-family: var(--font-brand);
    font-size: 20px;
    font-weight: 600;
    margin-bottom: 8px;
    color: #FFFFFF;
  }

  .countdown-text p {
    font-size: 14px;
    color: var(--text-muted);
    line-height: 1.5;
    margin-bottom: 24px;
  }

  .btn-cancel {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #FFFFFF;
    padding: 8px 24px;
    border-radius: 9999px;
    font-size: 13.5px;
    cursor: pointer;
    transition: background 0.18s ease;
  }

  .btn-cancel:hover {
    background: rgba(255, 255, 255, 0.16);
  }

  @keyframes pulseScale {
    0% { transform: scale(0.96); }
    50% { transform: scale(1.04); }
    100% { transform: scale(0.96); }
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
