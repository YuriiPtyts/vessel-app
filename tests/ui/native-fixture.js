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
    const collectionItem = {
      id: 'VLprovider_mix',
      kind: 'playlist',
      title: 'Provider Mix',
      subtitle: 'Selected by YouTube Music',
      thumbnailUrl: tracks[1].thumbnailUrl,
      track: null,
    };
    let library = { liked: options.deviceLikes ? [tracks[0]] : [], recent: [] };
    const savedPlaylist = {
      ...collectionItem,
      id: 'VLaccount_saved',
      title: 'Saved discoveries',
      subtitle: 'Your YouTube Music playlist',
    };
    let remoteLiked = [tracks[1]];
    let libraryRevision = 0;
    let accountLibrary = null;
    function accountLibrarySnapshot(ready = true) {
      return {
        revision: ++libraryRevision,
        accountRevision: account.revision,
        connected: account.mode === 'connected',
        refreshing: false,
        likesReady: ready,
        playlistsReady: ready,
        liked: ready ? structuredClone(remoteLiked) : [],
        likedIds: ready ? remoteLiked.map((track) => track.id) : [],
        playlists: ready ? [savedPlaylist] : [],
        likesError: null,
        playlistsError: null,
      };
    }
    let snapshot = {
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
    let inspection = null;
    let lastAudibleVolume = 40;
    let queue = {
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
    let account = { mode: 'guest', profile: null, error: null, revision: 0 };
    const callbacks = new Map();
    const listeners = new Map();
    function emit(event, payload) {
      if (event === 'queue-state' && payload === queue) queue.revision++;
      listeners.get(event)?.({ event, id: 1, payload: structuredClone(payload) });
    }
    function startSong(track) {
      snapshot = {
        ...snapshot,
        playbackId: snapshot.playbackId + 1,
        status: 'loading',
        track,
        activeFormat: formats[0],
        positionSeconds: 0,
        durationSeconds: track.durationSeconds,
      };
      inspection = { track, formats, recommendedFormatId: '251', accountSource: false };
      const playbackId = snapshot.playbackId;
      emit('queue-state', queue);
      emit('selection-state', inspection);
      emit('playback-state', snapshot);
      const selection = {
        inspection: structuredClone(inspection),
        playback: structuredClone(snapshot),
      };
      setTimeout(() => {
        if (snapshot.playbackId !== playbackId || !snapshot.track) return;
        snapshot = { ...snapshot, status: 'playing', decodedSampleRateHz: 48000 };
        library.recent = [track, ...library.recent.filter((old) => old.id !== track.id)];
        emit('playback-state', snapshot);
      }, 50);
      return selection;
    }
    function stop() {
      queue = {
        ...queue,
        running: false,
        transitioning: false,
        generating: false,
        preparingId: null,
      };
      snapshot = {
        ...snapshot,
        status: 'idle',
        track: null,
        activeFormat: null,
        positionSeconds: 0,
        durationSeconds: null,
        decodedSampleRateHz: null,
      };
      emit('queue-state', queue);
      emit('playback-state', snapshot);
    }
    window.isTauri = true;
    window.__vesselCalls = [];
    window.__vesselLibraryReplies = 0;
    window.__vesselVolumeInFlight = 0;
    window.__vesselMaxVolumeInFlight = 0;
    window.__vesselEmitSnapshot = (override) =>
      emit('playback-state', { ...snapshot, ...override });
    window.__vesselEmitQueue = (override) => emit('queue-state', { ...queue, ...override });
    window.__vesselAdvanceRadio = () => {
      queue.currentIndex++;
      return startSong(queue.tracks[queue.currentIndex]);
    };
    window.__vesselCompleteSignIn = () => {
      account = {
        mode: 'connected',
        profile: { name: 'Fixture Listener', email: 'listener@example.test', pictureUrl: null },
        error: null,
        revision: account.revision + 1,
      };
      emit('account-state', account);
    };
    window.__vesselExpireAccount = () => {
      account = {
        ...account,
        mode: 'expired',
        error: {
          code: 'session_expired',
          message: 'Reconnect Google to use your account again.',
          retryable: false,
        },
        revision: account.revision + 1,
      };
      emit('account-state', account);
    };
    window.__vesselSetRemoteLikes = (indices) => {
      remoteLiked = indices.map((index) => tracks[index]);
    };
    window.__vesselEmitAccountLibrary = (payload) => emit('account-library-state', payload);
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
            listeners.set(args.event, callbacks.get(args.handler));
            return 1;
          case 'plugin:event|unlisten':
            return;
          case 'set_material_mode':
            return;
          case 'playback_snapshot':
            return structuredClone(snapshot);
          case 'queue_snapshot':
            return structuredClone(queue);
          case 'account_snapshot':
            return structuredClone(account);
          case 'selection_snapshot':
            return structuredClone(inspection);
          case 'local_library':
            return structuredClone(library);
          case 'account_library':
            if (accountLibrary?.accountRevision !== account.revision)
              accountLibrary = accountLibrarySnapshot(false);
            return structuredClone(accountLibrary);
          case 'refresh_account_library': {
            const reply = accountLibrarySnapshot();
            if (options.libraryDelay)
              await new Promise((resolve) => setTimeout(resolve, options.libraryDelay));
            if (options.likesFailure) {
              reply.likesReady = false;
              reply.liked = [];
              reply.likedIds = [];
              reply.likesError = {
                code: 'account_library_unavailable',
                message: 'Your YouTube Music likes could not sync. Refresh to retry.',
                retryable: true,
              };
            }
            accountLibrary = reply;
            emit('account-library-state', reply);
            window.__vesselLibraryReplies++;
            return structuredClone(reply);
          }
          case 'set_account_like': {
            if (options.likeDelay)
              await new Promise((resolve) => setTimeout(resolve, options.likeDelay));
            if (options.likeFailure)
              throw {
                code: 'account_like_failed',
                message: 'YouTube Music did not confirm this like change.',
                retryable: true,
              };
            const track = tracks.find((track) => track.id === args.videoId);
            remoteLiked = remoteLiked.filter((track) => track.id !== args.videoId);
            if (args.liked) remoteLiked.unshift(track);
            accountLibrary = accountLibrarySnapshot();
            emit('account-library-state', accountLibrary);
            return structuredClone(accountLibrary);
          }
          case 'account_playlist':
            return {
              item: savedPlaylist,
              tracks: structuredClone(args.more ? tracks : tracks.slice(0, 2)),
              hasMore: !args.more,
            };
          case 'home_feed':
            return {
              personalized: account.mode === 'connected',
              shelves: [
                {
                  id: 'provider-picks',
                  title: options.homeTitle ?? 'Quick picks',
                  items: tracks.map((track) => ({
                    id: track.id,
                    kind: 'song',
                    title: track.title,
                    subtitle: track.artist,
                    thumbnailUrl: track.thumbnailUrl,
                    track,
                  })),
                },
                { id: 'provider-collections', title: 'Take it easy', items: [collectionItem] },
              ],
            };
          case 'browse_collection':
            return { item: collectionItem, tracks: structuredClone(tracks), hasMore: false };
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
            const context = args.context?.length
              ? args.context.map((id) => tracks.find((track) => track.id === id))
              : [track, ...tracks.filter((other) => other.id !== track.id)];
            queue = {
              ...queue,
              tracks: context,
              currentIndex: context.findIndex((item) => item.id === track.id),
              running: true,
              seedTitle: track.title,
            };
            return startSong(track);
          }
          case 'next_song':
            queue.currentIndex++;
            return startSong(queue.tracks[queue.currentIndex]);
          case 'previous_song':
            if (snapshot.positionSeconds > 3 || queue.currentIndex === 0) {
              snapshot.positionSeconds = 0;
              emit('playback-state', snapshot);
              return null;
            }
            queue.currentIndex--;
            return startSong(queue.tracks[queue.currentIndex]);
          case 'jump_queue':
            queue.currentIndex = args.index;
            queue.running = true;
            return startSong(queue.tracks[args.index]);
          case 'remove_queue':
            queue.tracks.splice(args.index, 1);
            if (args.index < queue.currentIndex) queue.currentIndex--;
            emit('queue-state', queue);
            return structuredClone(queue);
          case 'set_autoplay':
            queue.autoplay = args.enabled;
            emit('queue-state', queue);
            return structuredClone(queue);
          case 'retry_radio':
            queue.error = null;
            emit('queue-state', queue);
            return structuredClone(queue);
          case 'clear_queue':
            stop();
            queue = { ...queue, tracks: [], currentIndex: null, seedTitle: null };
            inspection = null;
            emit('queue-state', queue);
            emit('selection-state', null);
            return structuredClone(queue);
          case 'connect_google':
            account.mode = options.googleRejected ? 'unavailable' : 'signingIn';
            account.error = options.googleRejected
              ? {
                  code: 'google_sign_in_unavailable',
                  message:
                    'Google declined sign-in in this window. You can keep listening as a guest.',
                  retryable: false,
                }
              : null;
            emit('account-state', account);
            return structuredClone(account);
          case 'cancel_google':
            account.mode = account.profile ? 'connected' : 'guest';
            emit('account-state', account);
            return structuredClone(account);
          case 'sign_out':
            stop();
            account = { mode: 'guest', profile: null, error: null, revision: account.revision + 1 };
            inspection = null;
            emit('selection-state', null);
            emit('account-state', account);
            return structuredClone(account);
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
            if (args.volume > 0) lastAudibleVolume = args.volume;
            snapshot.volumeRevision++;
            window.__vesselVolumeInFlight--;
            break;
          case 'toggle_mute':
            snapshot.volume = snapshot.volume > 0 ? 0 : lastAudibleVolume;
            snapshot.volumeRevision++;
            break;
          case 'cancel_inspection':
            return;
          case 'stop_playback':
            stop();
            break;
          default:
            throw new Error(`Unexpected command: ${command}`);
        }
        return structuredClone(snapshot);
      },
    };
  }, options);
}
