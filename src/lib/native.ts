import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface RuntimeReport {
  appVersion: string;
  platform: string;
  architecture: string;
  debugBuild: boolean;
  playbackDependenciesAvailable: boolean;
}

export interface AppError {
  code: string;
  message: string;
  retryable: boolean;
}

export interface AudioFormat {
  id: string;
  codec: string;
  container: string;
  bitrateKbps: number | null;
  sampleRateHz: number | null;
  channels: number | null;
}

export interface Track {
  id: string;
  title: string;
  artist: string;
  durationSeconds: number | null;
  album: string | null;
  thumbnailUrl: string | null;
}

export type SearchKind = 'songs' | 'videos';
export interface SearchResults {
  query: string;
  kind: SearchKind;
  tracks: Track[];
  hasMore: boolean;
}
export interface HomeItem {
  id: string;
  kind: 'song' | 'video' | 'album' | 'playlist';
  title: string;
  subtitle: string;
  thumbnailUrl: string | null;
  track: Track | null;
}
export interface HomeFeed {
  shelves: { id: string; title: string; items: HomeItem[] }[];
  personalized: boolean;
}
export interface CollectionPage {
  item: HomeItem;
  tracks: Track[];
  hasMore: boolean;
}
export interface LocalLibrary {
  liked: Track[];
  recent: Track[];
}
export interface AccountLibrarySnapshot {
  revision: number;
  accountRevision: number;
  connected: boolean;
  refreshing: boolean;
  likesReady: boolean;
  playlistsReady: boolean;
  liked: Track[];
  likedIds: string[];
  playlists: HomeItem[];
  editablePlaylistIds: string[];
  likesError: AppError | null;
  playlistsError: AppError | null;
}
export const emptyAccountLibrary = (accountRevision = 0): AccountLibrarySnapshot => ({
  revision: 0,
  accountRevision,
  connected: false,
  refreshing: false,
  likesReady: false,
  playlistsReady: false,
  liked: [],
  likedIds: [],
  playlists: [],
  editablePlaylistIds: [],
  likesError: null,
  playlistsError: null,
});
export interface PlaybackSelection {
  inspection: TrackInspection;
  playback: PlaybackSnapshot;
}

export interface TrackInspection {
  track: Track;
  formats: AudioFormat[];
  recommendedFormatId: string;
  accountSource: boolean;
}

export interface QueueSnapshot {
  revision: number;
  tracks: Track[];
  currentIndex: number | null;
  autoplay: boolean;
  running: boolean;
  generating: boolean;
  transitioning: boolean;
  preparingId: string | null;
  seedTitle: string | null;
  error: AppError | null;
}

export interface AccountSnapshot {
  mode: 'guest' | 'checking' | 'signingIn' | 'connected' | 'expired' | 'unavailable';
  profile: { name: string; email: string | null; pictureUrl: string | null } | null;
  error: AppError | null;
  revision: number;
}

export const emptyQueue: QueueSnapshot = {
  revision: 0,
  tracks: [],
  currentIndex: null,
  autoplay: true,
  running: false,
  generating: false,
  transitioning: false,
  preparingId: null,
  seedTitle: null,
  error: null,
};
export const guestAccount: AccountSnapshot = {
  mode: 'guest',
  profile: null,
  error: null,
  revision: 0,
};

export type PlaybackStatus =
  'idle' | 'loading' | 'playing' | 'paused' | 'buffering' | 'ended' | 'failed';

export interface PlaybackSnapshot {
  playbackId: number;
  accountSource: boolean;
  status: PlaybackStatus;
  track: Track | null;
  activeFormat: AudioFormat | null;
  positionSeconds: number;
  durationSeconds: number | null;
  volume: number;
  volumeRevision: number;
  error: AppError | null;
  videoTrackSelected: boolean;
  decodedSampleRateHz: number | null;
}

export const emptyPlayback: PlaybackSnapshot = {
  playbackId: 0,
  accountSource: false,
  status: 'idle',
  track: null,
  activeFormat: null,
  positionSeconds: 0,
  durationSeconds: null,
  volume: 40,
  volumeRevision: 0,
  error: null,
  videoTrackSelected: false,
  decodedSampleRateHz: null,
};

export async function getRuntimeReport(): Promise<RuntimeReport | null> {
  if (!isTauri()) return null;
  return invoke<RuntimeReport>('runtime_report');
}

export const inspectTrack = (url: string) => invoke<TrackInspection>('inspect_track', { url });
export const searchMusic = (query: string, kind: SearchKind = 'songs', more = false) =>
  invoke<SearchResults>('search_music', { query, kind, more });
export const getHomeFeed = (refresh = false) => invoke<HomeFeed>('home_feed', { refresh });
export const browseCollection = (id: string, more = false) =>
  invoke<CollectionPage>('browse_collection', { id, more });
export const playSong = (videoId: string, context: string[] = []) =>
  invoke<PlaybackSelection>('play_song', { videoId, context });
export const getLibrary = () => invoke<LocalLibrary>('local_library');
export const getAccountLibrary = () => invoke<AccountLibrarySnapshot>('account_library');
export const refreshAccountLibrary = (accountRevision: number, force = false) =>
  invoke<AccountLibrarySnapshot>('refresh_account_library', { accountRevision, force });
export const setAccountLike = (videoId: string, liked: boolean, accountRevision: number) =>
  invoke<AccountLibrarySnapshot>('set_account_like', { videoId, liked, accountRevision });
export const browseAccountPlaylist = (id: string, accountRevision: number, more = false) =>
  invoke<CollectionPage>('account_playlist', { id, accountRevision, more });
export type PlaylistPrivacy = 'PRIVATE' | 'UNLISTED' | 'PUBLIC';
export interface PlaylistMutation {
  playlist: HomeItem;
  library: AccountLibrarySnapshot;
}
export interface SongLyrics {
  videoId: string;
  text: string | null;
  source: string | null;
}
export const createAccountPlaylist = (
  title: string,
  description: string,
  privacy: PlaylistPrivacy,
  videoId: string | null,
  accountRevision: number,
) =>
  invoke<PlaylistMutation>('create_account_playlist', {
    title,
    description,
    privacy,
    videoId,
    accountRevision,
  });
export const addToAccountPlaylist = (id: string, videoId: string, accountRevision: number) =>
  invoke<PlaylistMutation>('add_to_account_playlist', { id, videoId, accountRevision });
export const getSongLyrics = (videoId: string) => invoke<SongLyrics>('song_lyrics', { videoId });
export const onAccountLibrary = (callback: (snapshot: AccountLibrarySnapshot) => void) =>
  listen<AccountLibrarySnapshot>('account-library-state', ({ payload }) => callback(payload));
export const setMaterialMode = (reduced: boolean) => invoke<void>('set_material_mode', { reduced });
export const cancelInspection = () => invoke<void>('cancel_inspection');
export const playTrack = (trackId: string, formatId: string) =>
  invoke<PlaybackSnapshot>('play_track', { trackId, formatId });
export const getPlayback = () => invoke<PlaybackSnapshot>('playback_snapshot');
export const setPaused = (paused: boolean) => invoke<PlaybackSnapshot>('set_paused', { paused });
export const seekTrack = (position: number) => invoke<PlaybackSnapshot>('seek_track', { position });
export const setVolume = (volume: number) => invoke<PlaybackSnapshot>('set_volume', { volume });
export const toggleMute = () => invoke<PlaybackSnapshot>('toggle_mute');
export const stopPlayback = () => invoke<PlaybackSnapshot>('stop_playback');
export const onPlayback = (callback: (snapshot: PlaybackSnapshot) => void) =>
  listen<PlaybackSnapshot>('playback-state', ({ payload }) => callback(payload));
export const getQueue = () => invoke<QueueSnapshot>('queue_snapshot');
export const getSelection = () => invoke<TrackInspection | null>('selection_snapshot');
export const getAccount = () => invoke<AccountSnapshot>('account_snapshot');
export const nextSong = () => invoke<PlaybackSelection | null>('next_song');
export const previousSong = () => invoke<PlaybackSelection | null>('previous_song');
export const jumpQueue = (index: number, videoId: string) =>
  invoke<PlaybackSelection>('jump_queue', { index, videoId });
export const removeQueue = (index: number, videoId: string) =>
  invoke<QueueSnapshot>('remove_queue', { index, videoId });
export const clearQueue = () => invoke<QueueSnapshot>('clear_queue');
export const setAutoplay = (enabled: boolean) => invoke<QueueSnapshot>('set_autoplay', { enabled });
export const retryRadio = () => invoke<QueueSnapshot>('retry_radio');
export const connectGoogle = () => invoke<AccountSnapshot>('connect_google');
export const cancelGoogle = () => invoke<AccountSnapshot>('cancel_google');
export const signOut = () => invoke<AccountSnapshot>('sign_out');
export const onQueue = (callback: (snapshot: QueueSnapshot) => void) =>
  listen<QueueSnapshot>('queue-state', ({ payload }) => callback(payload));
export const onSelection = (callback: (inspection: TrackInspection | null) => void) =>
  listen<TrackInspection | null>('selection-state', ({ payload }) => callback(payload));
export const onAccount = (callback: (snapshot: AccountSnapshot) => void) =>
  listen<AccountSnapshot>('account-state', ({ payload }) => callback(payload));

export function errorMessage(error: unknown): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof error.message === 'string'
  ) {
    return error.message;
  }
  return 'The desktop connection could not complete this operation. Restart Vessel and try again.';
}

export function formatLabel(format: AudioFormat): string {
  const codec =
    format.codec === 'opus'
      ? 'Opus'
      : format.codec === 'mp4a.40.5'
        ? 'HE-AAC'
        : format.codec.startsWith('mp4a')
          ? 'AAC'
          : format.codec;
  const bitrate =
    format.bitrateKbps === null ? 'bitrate unknown' : `≈${Math.round(format.bitrateKbps)} kbps`;
  return `${codec} · ${bitrate} · ${format.container.toUpperCase()}`;
}

export function trackTime(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return '—:—';
  const value = Math.max(0, Math.floor(seconds));
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
}

export function isCancelled(error: unknown): boolean {
  return (
    typeof error === 'object' && error !== null && 'code' in error && error.code === 'cancelled'
  );
}
