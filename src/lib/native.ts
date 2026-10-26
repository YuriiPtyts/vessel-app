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
}

export interface TrackInspection {
  track: Track;
  formats: AudioFormat[];
  recommendedFormatId: string;
}

export type PlaybackStatus = 'idle' | 'loading' | 'playing' | 'paused' | 'buffering' | 'ended' | 'failed';

export interface PlaybackSnapshot {
  status: PlaybackStatus;
  track: Track | null;
  activeFormat: AudioFormat | null;
  positionSeconds: number;
  durationSeconds: number | null;
  volume: number;
  error: AppError | null;
  videoTrackSelected: boolean;
  decodedSampleRateHz: number | null;
}

export const emptyPlayback: PlaybackSnapshot = {
  status: 'idle', track: null, activeFormat: null, positionSeconds: 0,
  durationSeconds: null, volume: 40, error: null, videoTrackSelected: false, decodedSampleRateHz: null,
};

export async function getRuntimeReport(): Promise<RuntimeReport | null> {
  if (!isTauri()) return null;
  return invoke<RuntimeReport>('runtime_report');
}

export const inspectTrack = (url: string) => invoke<TrackInspection>('inspect_track', { url });
export const cancelInspection = () => invoke<void>('cancel_inspection');
export const playTrack = (trackId: string, formatId: string) => invoke<PlaybackSnapshot>('play_track', { trackId, formatId });
export const getPlayback = () => invoke<PlaybackSnapshot>('playback_snapshot');
export const setPaused = (paused: boolean) => invoke<PlaybackSnapshot>('set_paused', { paused });
export const seekTrack = (position: number) => invoke<PlaybackSnapshot>('seek_track', { position });
export const setVolume = (volume: number) => invoke<PlaybackSnapshot>('set_volume', { volume });
export const stopPlayback = () => invoke<PlaybackSnapshot>('stop_playback');
export const onPlayback = (callback: (snapshot: PlaybackSnapshot) => void) =>
  listen<PlaybackSnapshot>('playback-state', ({ payload }) => callback(payload));

export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  return 'The desktop connection could not complete this operation. Restart Vessel and try again.';
}

export function formatLabel(format: AudioFormat): string {
  const codec = format.codec === 'opus' ? 'Opus'
    : format.codec === 'mp4a.40.5' ? 'HE-AAC'
    : format.codec.startsWith('mp4a') ? 'AAC' : format.codec;
  const bitrate = format.bitrateKbps === null ? 'bitrate unknown' : `≈${Math.round(format.bitrateKbps)} kbps`;
  return `${codec} · ${bitrate} · ${format.container.toUpperCase()}`;
}
