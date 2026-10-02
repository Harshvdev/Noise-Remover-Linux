<script lang="ts">
  import { api } from './api';
  import { Minus, Square, X } from '@lucide/svelte';

  const handleMinimize = (e: MouseEvent) => {
    e.stopPropagation();
    api.windowMinimize();
  };
  const handleMaximize = (e: MouseEvent) => {
    e.stopPropagation();
    api.windowMaximize();
  };
  const handleClose = (e: MouseEvent) => {
    e.stopPropagation();
    api.windowClose();
  };
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<header class="titlebar" data-tauri-drag-region ondblclick={handleMaximize}>
  <div class="brand" data-tauri-drag-region>
    <!-- Waveform Logo Icon -->
    <div class="logo-bars" data-tauri-drag-region>
      <span class="bar bar-1"></span>
      <span class="bar bar-2"></span>
      <span class="bar bar-3"></span>
      <span class="bar bar-4"></span>
      <span class="bar bar-5"></span>
    </div>
    <span class="app-title" data-tauri-drag-region>Voice Cleaner</span>
  </div>

  <div class="window-controls">
    <button class="win-btn" onclick={handleMinimize} title="Minimize" aria-label="Minimize">
      <Minus size={13} strokeWidth={2.2} />
    </button>
    <button class="win-btn" onclick={handleMaximize} title="Maximize / Restore" aria-label="Maximize or Restore">
      <Square size={11} strokeWidth={2.2} />
    </button>
    <button class="win-btn win-btn-close" onclick={handleClose} title="Close" aria-label="Close">
      <X size={13} strokeWidth={2.2} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: 38px;
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    background: var(--bg-app);
    border-bottom: 1px solid var(--border-subtle);
    z-index: 100;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: default;
  }

  .logo-bars {
    display: flex;
    align-items: center;
    gap: 2.5px;
    height: 16px;
  }

  .bar {
    width: 2.5px;
    background-color: var(--accent-lime);
    border-radius: 9999px;
  }

  .bar-1 { height: 7px; }
  .bar-2 { height: 14px; }
  .bar-3 { height: 16px; }
  .bar-4 { height: 11px; }
  .bar-5 { height: 6px; }

  .app-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-main);
    letter-spacing: -0.2px;
  }

  .window-controls {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .win-btn {
    width: 28px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .win-btn:hover {
    background: var(--bg-card-hover);
    color: var(--text-main);
  }

  .win-btn-close:hover {
    background: #E81123;
    color: #FFFFFF;
  }
</style>
