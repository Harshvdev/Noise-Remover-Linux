<script lang="ts">
  import Titlebar from './lib/Titlebar.svelte';
  import PanelHome from './lib/PanelHome.svelte';
  import PanelRecordings from './lib/PanelRecordings.svelte';
  import PanelTrackDetail from './lib/PanelTrackDetail.svelte';
  import SettingsDrawer from './lib/SettingsDrawer.svelte';
  import DiagnosticsDrawer from './lib/DiagnosticsDrawer.svelte';
  import SplashScreen from './lib/SplashScreen.svelte';
  import type { ViewScreen } from './lib/types';
  import { currentTheme, applyTheme } from './lib/theme';
  import { onMount } from 'svelte';

  let showSplash = true;
  let currentView: ViewScreen = 'home';
  let activeTrackId = 'track_01';
  let showSettings = false;
  let diagnosticsTrackId: string | null = null;

  function navigateToHome() {
    currentView = 'home';
    showSettings = false;
    diagnosticsTrackId = null;
  }

  function navigateToRecordings() {
    currentView = 'recordings';
    showSettings = false;
    diagnosticsTrackId = null;
  }

  function navigateToTrackDetail(trackId: string) {
    activeTrackId = trackId;
    currentView = 'track_detail';
    showSettings = false;
    diagnosticsTrackId = null;
  }

  function openSettings() {
    showSettings = true;
  }

  function closeSettings() {
    showSettings = false;
  }

  function openDiagnostics(trackId: string) {
    diagnosticsTrackId = trackId;
  }

  function closeDiagnostics() {
    diagnosticsTrackId = null;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
    if (e.key === '1') {
      navigateToHome();
    } else if (e.key === '2') {
      navigateToRecordings();
    } else if (e.key === '3') {
      navigateToTrackDetail(activeTrackId || 'track_01');
    } else if (e.key === '4') {
      showSettings = !showSettings;
      diagnosticsTrackId = null;
    } else if (e.key === '5') {
      diagnosticsTrackId = diagnosticsTrackId ? null : (activeTrackId || 'track_01');
      showSettings = false;
    } else if (e.key === 'Escape') {
      showSettings = false;
      diagnosticsTrackId = null;
    }
  }

  onMount(() => {
    applyTheme($currentTheme);
    const handleHash = () => {
      const h = window.location.hash.toLowerCase();
      if (h.includes('recordings')) navigateToRecordings();
      else if (h.includes('detail')) navigateToTrackDetail(activeTrackId || 'track_01');
      else if (h.includes('settings')) openSettings();
      else if (h.includes('diagnostics')) openDiagnostics(activeTrackId || 'track_01');
      else if (h.includes('home')) navigateToHome();
    };
    handleHash();
    window.addEventListener('hashchange', handleHash);
    return () => window.removeEventListener('hashchange', handleHash);
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

<div class="app-root">
  <!-- Active Screen Views -->
  <div class="view-viewport">
    {#if currentView === 'home'}
      <PanelHome
        onNavigateRecordings={navigateToRecordings}
        onNavigateTrackDetail={navigateToTrackDetail}
        onOpenSettings={openSettings}
      />
    {:else if currentView === 'recordings'}
      <PanelRecordings
        onNavigateHome={navigateToHome}
        onNavigateTrackDetail={navigateToTrackDetail}
        onOpenSettings={openSettings}
      />
    {:else if currentView === 'track_detail'}
      <PanelTrackDetail
        trackId={activeTrackId}
        onNavigateBack={navigateToRecordings}
        onOpenDiagnostics={openDiagnostics}
      />
    {/if}
  </div>

  <!-- Drawers / Overlays -->
  {#if showSettings}
    <SettingsDrawer onClose={closeSettings} />
  {/if}

  {#if diagnosticsTrackId}
    <DiagnosticsDrawer
      trackId={diagnosticsTrackId}
      onClose={closeDiagnostics}
    />
  {/if}

  {#if showSplash}
    <SplashScreen onFinish={() => (showSplash = false)} />
  {/if}
</div>

<style>
  .app-root {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background-color: var(--bg-app);
    overflow: hidden;
    position: relative;
    overscroll-behavior: none;
    touch-action: manipulation;
  }

  .view-viewport {
    flex: 1;
    min-height: 0;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
    overscroll-behavior: none;
  }
</style>
