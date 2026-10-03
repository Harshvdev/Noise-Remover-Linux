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
  import { api } from './lib/api';
  import { onMount } from 'svelte';

  // Prevent splash screen replay if user is switching back from recents/background
  let showSplash = typeof window !== 'undefined' ? !sessionStorage.getItem('vc_splash_done') : true;

  // Persist last active view so activity recreation doesn't lose the user's screen
  let currentView: ViewScreen = (typeof window !== 'undefined' && (sessionStorage.getItem('vc_view') as ViewScreen)) || 'home';
  let activeTrackId = (typeof window !== 'undefined' && sessionStorage.getItem('vc_track_id')) || 'track_01';
  let showSettings = false;
  let diagnosticsTrackId: string | null = null;

  function onSplashFinish() {
    showSplash = false;
    try {
      sessionStorage.setItem('vc_splash_done', '1');
    } catch (_) {}
  }

  function navigateToHome() {
    currentView = 'home';
    showSettings = false;
    diagnosticsTrackId = null;
    try {
      sessionStorage.setItem('vc_view', 'home');
    } catch (_) {}
  }

  function navigateToRecordings() {
    currentView = 'recordings';
    showSettings = false;
    diagnosticsTrackId = null;
    try {
      sessionStorage.setItem('vc_view', 'recordings');
    } catch (_) {}
  }

  function navigateToTrackDetail(trackId: string) {
    activeTrackId = trackId;
    currentView = 'track_detail';
    showSettings = false;
    diagnosticsTrackId = null;
    try {
      sessionStorage.setItem('vc_view', 'track_detail');
      sessionStorage.setItem('vc_track_id', trackId);
    } catch (_) {}
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

  function exitApp() {
    try {
      if ((window as any).AndroidBridge?.exitApp) {
        (window as any).AndroidBridge.exitApp();
        return;
      }
    } catch (e) {
      console.warn('AndroidBridge exit error:', e);
    }
    api.windowClose().catch((e) => console.warn('windowClose error:', e));
  }

  export function handleAppBack() {
    // 1. If settings drawer open, close it
    if (showSettings) {
      showSettings = false;
      return;
    }
    // 2. If diagnostics drawer open, close it
    if (diagnosticsTrackId) {
      diagnosticsTrackId = null;
      return;
    }
    // 3. Dispatch cancelable event to let child panel handle sub-actions (e.g. More menu, renaming)
    const ev = new CustomEvent('app-back-press', { cancelable: true });
    const isHandled = !window.dispatchEvent(ev);
    if (isHandled) {
      return;
    }

    // 4. Panel navigation rules:
    // Panel 3 (track_detail) -> Panel 2 (recordings)
    // Panel 2 (recordings) -> Panel 1 (home)
    // Panel 1 (home) -> exit app
    if (currentView === 'track_detail') {
      navigateToRecordings();
    } else if (currentView === 'recordings') {
      navigateToHome();
    } else if (currentView === 'home') {
      exitApp();
    }
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
      handleAppBack();
    }
  }

  onMount(() => {
    applyTheme($currentTheme);

    const onAndroidBack = () => handleAppBack();
    window.addEventListener('android-back-button', onAndroidBack);
    (window as any).__handleAndroidBack = handleAppBack;

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

    return () => {
      window.removeEventListener('hashchange', handleHash);
      window.removeEventListener('android-back-button', onAndroidBack);
      delete (window as any).__handleAndroidBack;
    };
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
    <SplashScreen onFinish={onSplashFinish} />
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
