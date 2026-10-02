<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from './api';
  import type { TrackMetadata, PlaybackStatusDto } from './types';
  import TrackRow from './TrackRow.svelte';
  import { ChevronLeft, Settings, LayoutGrid, Star, Calendar, ChevronDown } from '@lucide/svelte';

  export let onNavigateHome: () => void;
  export let onNavigateTrackDetail: (trackId: string) => void;
  export let onOpenSettings: () => void;

  let tracks: TrackMetadata[] = [];
  let filter: 'all' | 'favorites' = 'all';
  let dateSortDesc = true;

  let activeTrackId: string | null = null;
  let isPlaying = false;
  let currentPosSecs = 0;
  let statusPollInterval: ReturnType<typeof setInterval>;
  let animFrameId: number;
  let lastFrameTime = performance.now();

  let isScrubbing = false;
  let pendingSeekExpires = 0;

  async function loadTracks() {
    try {
      tracks = await api.getTracks();
    } catch (e) {
      console.error('Failed to load tracks:', e);
    }
  }

  function updatePlayhead() {
    const now = performance.now();
    const dt = (now - lastFrameTime) / 1000;
    lastFrameTime = now;

    if (isPlaying && !isScrubbing && activeTrackId) {
      const curTrack = tracks.find((t) => t.id === activeTrackId);
      const maxDur = curTrack?.duration_secs || 0;
      if (maxDur > 0) {
        currentPosSecs = Math.min(maxDur, currentPosSecs + dt);
        if (currentPosSecs >= maxDur) {
          isPlaying = false;
        }
      }
    }

    animFrameId = requestAnimationFrame(updatePlayhead);
  }

  onMount(() => {
    loadTracks();
    lastFrameTime = performance.now();
    animFrameId = requestAnimationFrame(updatePlayhead);

    statusPollInterval = setInterval(async () => {
      if (isScrubbing) return;
      try {
        const status = await api.getPlaybackStatus();
        if (isScrubbing) return;

        if (status.track_id) {
          if (activeTrackId !== status.track_id) {
            activeTrackId = status.track_id;
            currentPosSecs = status.position_seconds;
          }
          isPlaying = status.is_playing;

          // Reject stale pre-seek status reports from previously in-flight queries
          if (pendingSeekExpires > Date.now()) {
            const diff = Math.abs(status.position_seconds - currentPosSecs);
            if (diff <= 0.75) {
              // Backend has caught up with seek
              pendingSeekExpires = 0;
              currentPosSecs = status.position_seconds;
            }
          } else {
            // Reconcile if paused or if subtle drift exceeds 0.2s
            if (!isPlaying || Math.abs(status.position_seconds - currentPosSecs) > 0.2) {
              currentPosSecs = status.position_seconds;
            }
          }
        } else if (!status.is_playing) {
          isPlaying = false;
        }
      } catch (e) {
        // Fallback or ignore
      }
    }, 100);
  });

  onDestroy(() => {
    if (statusPollInterval) clearInterval(statusPollInterval);
    if (animFrameId) cancelAnimationFrame(animFrameId);
  });

  async function handlePlayToggle(track: TrackMetadata, e: MouseEvent) {
    e.stopPropagation();
    if (activeTrackId === track.id && isPlaying) {
      await api.pauseTrack();
      isPlaying = false;
    } else {
      lastFrameTime = performance.now();
      await api.playTrack(track.id, true, currentPosSecs > 0 && activeTrackId === track.id ? currentPosSecs : undefined);
      activeTrackId = track.id;
      isPlaying = true;
    }
  }

  async function handleSeek(track: TrackMetadata, sec: number, isFinal: boolean) {
    const isNewTrack = activeTrackId !== track.id;
    isScrubbing = !isFinal;
    activeTrackId = track.id;
    currentPosSecs = sec;
    lastFrameTime = performance.now();

    if (isFinal) {
      pendingSeekExpires = Date.now() + 800;
      if (isNewTrack) {
        await api.playTrack(track.id, true, sec);
        isPlaying = true;
      } else {
        await api.seekTrack(sec);
      }
    }
  }

  async function handleFavoriteToggle(track: TrackMetadata, e: MouseEvent) {
    e.stopPropagation();
    try {
      const fav = await api.toggleFavorite(track.id);
      track.is_favorite = fav;
      tracks = [...tracks];
    } catch (e) {
      console.error(e);
    }
  }

  async function handleDelete(track: TrackMetadata, e: MouseEvent) {
    e.stopPropagation();
    try {
      await api.deleteTrack(track.id);
      tracks = tracks.filter((t) => t.id !== track.id);
      if (activeTrackId === track.id) {
        await api.pauseTrack();
        activeTrackId = null;
        isPlaying = false;
        currentPosSecs = 0;
      }
    } catch (e) {
      console.error(e);
    }
  }

  $: filteredTracks = tracks
    .filter((t) => (filter === 'favorites' ? t.is_favorite : true))
    .sort((a, b) => (dateSortDesc ? b.timestamp - a.timestamp : a.timestamp - b.timestamp));
</script>

<div class="panel-recordings">
  <!-- Top Bar -->
  <header class="recordings-topbar">
    <button class="btn-nav" onclick={onNavigateHome}>
      <ChevronLeft size={18} />
      <span>Back</span>
    </button>

    <button class="btn-nav" onclick={onOpenSettings}>
      <Settings size={18} color="var(--text-main)" />
      <span>Settings</span>
    </button>
  </header>

  <!-- Title & Filter Segmented Row -->
  <div class="header-action-row">
    <div class="title-group">
      <h1 class="page-title">Recordings</h1>
      <p class="page-subtitle">Manage your recordings, play, favorite or delete them.</p>
    </div>

    <!-- Segmented Filter Control matching panel-2.png -->
    <div class="filter-controls">
      <button
        class="filter-tab"
        class:active-tab={filter === 'all'}
        onclick={() => (filter = 'all')}
      >
        <LayoutGrid size={15} color={filter === 'all' ? 'var(--text-on-accent)' : 'var(--text-muted)'} />
        <span>All</span>
      </button>

      <button
        class="filter-tab"
        class:active-tab={filter === 'favorites'}
        onclick={() => (filter = 'favorites')}
      >
        <Star
          size={15}
          fill={filter === 'favorites' ? 'var(--text-on-accent)' : 'none'}
          color={filter === 'favorites' ? 'var(--text-on-accent)' : 'var(--text-muted)'}
          strokeWidth={1.8}
        />
        <span>Favorites</span>
      </button>

      <!-- Vertical Divider between Favorites and Date -->
      <span class="filter-divider"></span>

      <button
        class="filter-tab date-tab"
        onclick={() => (dateSortDesc = !dateSortDesc)}
        title="Sort by date"
      >
        <Calendar size={15} />
        <span>Date</span>
        <ChevronDown size={14} style="transform: rotate({dateSortDesc ? 0 : 180}deg); transition: transform 0.2s ease;" />
      </button>
    </div>
  </div>

  <!-- Track Rows List -->
  <div class="tracks-scroll-area">
    {#if filteredTracks.length === 0}
      <div class="empty-state">
        <p>No recordings found.</p>
      </div>
    {:else}
      <div class="tracks-list">
        {#each filteredTracks as track (track.id)}
          <TrackRow
            {track}
            isPlaying={activeTrackId === track.id && isPlaying}
            isCurrentTrack={activeTrackId === track.id}
            currentPosSecs={activeTrackId === track.id ? currentPosSecs : 0}
            onPlayToggle={(e) => handlePlayToggle(track, e)}
            onFavoriteToggle={(e) => handleFavoriteToggle(track, e)}
            onDelete={(e) => handleDelete(track, e)}
            onOpenDetail={() => onNavigateTrackDetail(track.id)}
            onSeek={(sec, isFinal) => handleSeek(track, sec, isFinal)}
          />
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .panel-recordings {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 24px 36px;
    background-color: var(--bg-app);
    overflow: hidden;
  }

  .recordings-topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 22px;
    width: 100%;
  }

  /* Squircle nav buttons matching panel-2.png */
  .btn-nav {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    color: var(--text-main);
    padding: 9px 18px;
    border-radius: 14px;
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: var(--shadow-card);
    transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .btn-nav:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-medium);
    transform: translateY(-1px);
  }

  .btn-nav:active {
    transform: translateY(0);
  }

  .header-action-row {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    margin-bottom: 22px;
    width: 100%;
    gap: 16px;
  }

  .title-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .page-title {
    font-family: var(--font-brand);
    font-size: 32px;
    font-weight: 700;
    color: var(--text-main);
    letter-spacing: -0.5px;
  }

  .page-subtitle {
    font-size: 14px;
    color: var(--text-muted);
  }

  .filter-controls {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    box-shadow: var(--shadow-card);
    padding: 4px 6px;
    border-radius: 14px;
  }

  .filter-tab {
    display: flex;
    align-items: center;
    gap: 7px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 7px 16px;
    border-radius: 10px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .filter-tab:hover:not(.active-tab) {
    color: var(--text-main);
    background: var(--bg-card-hover);
  }

  .filter-tab.active-tab {
    background: var(--accent-lime);
    color: var(--text-on-accent);
    font-weight: 700;
  }

  .filter-divider {
    width: 1px;
    height: 18px;
    background: var(--border-subtle);
    margin: 0 4px;
  }

  .date-tab {
    gap: 6px;
  }

  .tracks-scroll-area {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding-right: 4px;
  }

  .tracks-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-bottom: 20px;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: var(--text-muted);
    font-size: 15px;
  }

  @media (max-width: 780px) {
    .panel-recordings {
      padding: 18px 20px;
    }
    .header-action-row {
      flex-direction: column;
      align-items: flex-start;
      gap: 12px;
    }
  }
</style>
