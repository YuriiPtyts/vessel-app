export async function nativeFixture(page, options = {}) {
  await page.addInitScript((options) => {
    const cover = (color, name) =>
      `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300"><defs><linearGradient id="g" x2=".8" y2="1"><stop stop-color="${color}"/><stop offset="1" stop-color="#24232c"/></linearGradient></defs><rect width="300" height="300" fill="url(#g)"/><circle cx="160" cy="133" r="85" fill="none" stroke="#fff" stroke-opacity=".18" stroke-width="1"/><path d="M0 200Q100 70 300 210V300H0" fill="#dec6aa" fill-opacity=".2"/><text x="23" y="263" fill="#eee0cb" font-family="Georgia" font-size="19">${name}</text></svg>`)}`;
    const tracks = [
      {
        id: 'khnokW3Mw24',
        title: 'Instant Crush',
        artist: 'Daft Punk',
        album: 'Random Access Memories',
        durationSeconds: 338,
        thumbnailUrl: options.artworkUrl ?? cover('#92755d', 'Random Access Memories'),
      },
      {
        id: 'd7woYjGWs6k',
        title: 'Da Funk',
        artist: 'Daft Punk',
        album: 'Homework',
        durationSeconds: 329,
        thumbnailUrl: cover('#4e7678', 'Homework'),
      },
      {
        id: 'LxgnIhjQFuE',
        title: 'Harder, Better, Faster, Stronger',
        artist: 'Daft Punk',
        album: 'Alive 2007',
        durationSeconds: 205,
        thumbnailUrl: cover('#695a82', 'Alive 2007'),
      },
      {
        id: 'lw19BL_2e_Q',
        title: 'Around the World',
        artist: 'Daft Punk',
        album: 'Homework Remixed',
        durationSeconds: 471,
        thumbnailUrl: cover('#597a64', 'Homework Remixed'),
      },
      {
        id: '9WkE0L5TQT4',
        title: 'Something About Us',
        artist: 'Daft Punk',
        album: 'Discovery',
        durationSeconds: 232,
        thumbnailUrl: cover('#986b64', 'Discovery'),
      },
    ];
    const formats = [
      {
        id: '251',
        codec: 'opus',
        container: 'webm',
        bitrateKbps: 129,
        sampleRateHz: 48000,
        channels: 2,
      },
      {
        id: '140',
        codec: 'mp4a.40.2',
        container: 'm4a',
        bitrateKbps: 128,
        sampleRateHz: 44100,
        channels: 2,
      },
    ];
    let library = { liked: [], recent: [] };
    let snapshot = {
      status: 'idle',
      track: null,
      activeFormat: null,
      positionSeconds: 0,
      durationSeconds: null,
      volume: 40,
      error: null,
      videoTrackSelected: false,
      decodedSampleRateHz: null,
    };
    const callbacks = new Map();
    let listener;
    window.isTauri = true;
    window.__vesselCalls = [];
    window.__vesselVolumeInFlight = 0;
    window.__vesselMaxVolumeInFlight = 0;
    window.__vesselEmitSnapshot = (override) =>
      listener?.({
        event: 'playback-state',
        id: 1,
        payload: structuredClone({ ...snapshot, ...override }),
      });
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) {
        const id = callbacks.size + 1;
        callbacks.set(id, callback);
        return id;
      },
      async invoke(command, args) {
        window.__vesselCalls.push({ command, args });
        switch (command) {
          case 'runtime_report':
            return {
              appVersion: 'test',
              platform: 'macos',
              architecture: 'aarch64',
              debugBuild: true,
              playbackDependenciesAvailable: true,
            };
          case 'plugin:event|listen':
            listener = callbacks.get(args.handler);
            return 1;
          case 'plugin:event|unlisten':
            return;
          case 'set_material_mode':
            return;
          case 'playback_snapshot':
            return structuredClone(snapshot);
          case 'local_library':
            return structuredClone(library);
          case 'search_music': {
            if (args.query === 'older request')
              await new Promise((resolve) => setTimeout(resolve, 400));
            return {
              query: args.query,
              kind: args.kind,
              tracks: structuredClone(tracks),
              hasMore: !args.more,
            };
          }
          case 'play_song': {
            const track = tracks.find((track) => track.id === args.videoId);
            snapshot = {
              ...snapshot,
              status: 'loading',
              track,
              activeFormat: formats[0],
              durationSeconds: track.durationSeconds,
            };
            const response = {
              inspection: { track, formats, recommendedFormatId: '251' },
              playback: structuredClone(snapshot),
            };
            setTimeout(() => {
              snapshot = { ...snapshot, status: 'playing', decodedSampleRateHz: 48000 };
              library.recent = [track, ...library.recent.filter((old) => old.id !== track.id)];
              listener?.({ event: 'playback-state', id: 1, payload: structuredClone(snapshot) });
            }, 50);
            return response;
          }
          case 'play_track':
            snapshot.activeFormat = formats.find((format) => format.id === args.formatId);
            break;
          case 'toggle_like': {
            const track = tracks.find((track) => track.id === args.videoId);
            library.liked = library.liked.some((old) => old.id === track.id)
              ? library.liked.filter((old) => old.id !== track.id)
              : [track, ...library.liked];
            return structuredClone(library);
          }
          case 'set_paused':
            snapshot.status = args.paused ? 'paused' : 'playing';
            break;
          case 'seek_track':
            snapshot.positionSeconds = args.position;
            break;
          case 'set_volume':
            window.__vesselVolumeInFlight++;
            window.__vesselMaxVolumeInFlight = Math.max(
              window.__vesselMaxVolumeInFlight,
              window.__vesselVolumeInFlight,
            );
            if (options.volumeDelay)
              await new Promise((resolve) => setTimeout(resolve, options.volumeDelay));
            snapshot.volume = args.volume;
            window.__vesselVolumeInFlight--;
            break;
          case 'cancel_inspection':
            return;
          case 'stop_playback':
            snapshot = {
              ...snapshot,
              status: 'idle',
              track: null,
              activeFormat: null,
              positionSeconds: 0,
              durationSeconds: null,
              decodedSampleRateHz: null,
            };
            break;
          default:
            throw new Error(`Unexpected command: ${command}`);
        }
        return structuredClone(snapshot);
      },
    };
  }, options);
}
