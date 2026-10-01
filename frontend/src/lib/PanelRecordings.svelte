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

  let activeTrackId: string | null = 'track_01';
  let isPlaying = true;
  let currentPosSecs = 42;
  let statusPollInterval: ReturnType<typeof setInterval>;

  async function loadTracks() {
    try {
      tracks = await api.getTracks();
    } catch (e) {
      console.error('Failed to load tracks:', e);
    }
  }

  onMount(() => {
    loadTracks();

    statusPollInterval = setInterval(async () => {
      try {
        const status = await api.getPlaybackStatus();
        if (status.track_id) {
          isPlaying = status.is_playing;
          currentPosSecs = status.position_seconds;
          activeTrackId = status.track_id;
        }
      } catch (e) {
        // fallback in dev
        if (isPlaying) {
          currentPosSecs = (currentPosSecs + 0.2) % 134;
        }
      }
    }, 200);
  });

  onDestroy(() => {
    if (statusPollInterval) clearInterval(statusPollInterval);
  });

  async function handlePlayToggle(track: TrackMetadata, e: MouseEvent) {
    e.stopPropagation();
    if (activeTrackId === track.id && isPlaying) {
      await api.pauseTrack();
      isPlaying = false;
    } else {
      await api.playTrack(track.id, true);
      activeTrackId = track.id;
      isPlaying = true;
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
    <button class="btn-pill" onclick={onNavigateHome}>
      <ChevronLeft size={18} />
      <span>Back</span>
    </button>

    <button class="btn-pill" onclick={onOpenSettings}>
      <Settings size={17} color="rgba(255,255,255,0.85)" />
      <span>Settings</span>
    </button>
  </header>

  <!-- Title & Filter Segmented Row -->
  <div class="header-action-row">
    <div class="title-group">
      <h1 class="page-title">Recordings</h1>
      <p class="page-subtitle">Manage your recordings, play, favorite or delete them.</p>
    </div>

    <!-- Segmented Filter Control -->
    <div class="filter-controls">
      <button
        class="filter-pill"
        class:active-pill={filter === 'all'}
        onclick={() => (filter = 'all')}
      >
        <LayoutGrid size={15} />
        <span>All</span>
      </button>

      <button
        class="filter-pill"
        class:active-pill={filter === 'favorites'}
        onclick={() => (filter = 'favorites')}
      >
        <Star size={15} fill={filter === 'favorites' ? '#000000' : 'none'} />
        <span>Favorites</span>
      </button>

      <button
        class="filter-pill date-pill"
        onclick={() => (dateSortDesc = !dateSortDesc)}
        title="Sort by date"
      >
        <Calendar size={15} />
        <span>Date</span>
        <ChevronDown size={14} style="transform: rotate({dateSortDesc ? 0 : 180}deg);" />
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
            {currentPosSecs}
            onPlayToggle={(e) => handlePlayToggle(track, e)}
            onFavoriteToggle={(e) => handleFavoriteToggle(track, e)}
            onDelete={(e) => handleDelete(track, e)}
            onOpenDetail={() => onNavigateTrackDetail(track.id)}
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
    margin-bottom: 24px;
    width: 100%;
  }

  .header-action-row {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    margin-bottom: 24px;
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
    color: #FFFFFF;
    letter-spacing: -0.5px;
  }

  .page-subtitle {
    font-size: 14px;
    color: var(--text-muted);
  }

  .filter-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    background: #16161A;
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 4px 6px;
    border-radius: 9999px;
  }

  .filter-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    padding: 7px 14px;
    border-radius: 9999px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .filter-pill:hover {
    color: #FFFFFF;
  }

  .filter-pill.active-pill {
    background: var(--accent-lime);
    color: #000000;
    font-weight: 600;
    box-shadow: 0 0 14px var(--accent-lime-glow);
  }

  .date-pill {
    gap: 5px;
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
