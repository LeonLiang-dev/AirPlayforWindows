import { useSyncExternalStore } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { playbackStore } from '../../hooks/usePlayback';

export function PlaybackControls() {
  const { isPlaying, activeDeviceIds, volumes, error } = useSyncExternalStore(
    playbackStore.subscribe,
    () => playbackStore.snapshot,
  );

  const hasActiveDevices = activeDeviceIds.length > 0;

  const handlePlay = async () => {
    try {
      playbackStore.setError(null);
      playbackStore.setPlaying(true);
      await invoke('start_streaming', { deviceIds: activeDeviceIds });
    } catch (e) {
      console.error('Start streaming failed:', e);
      playbackStore.setError(String(e));
      playbackStore.setPlaying(false);
    }
  };

  const handleStop = async () => {
    try {
      await invoke('stop_streaming');
      playbackStore.setPlaying(false);
      playbackStore.setError(null);
    } catch (e) {
      console.error('Stop streaming failed:', e);
      playbackStore.setError(String(e));
    }
  };

  const handlePause = async () => {
    try {
      await invoke('pause_playback');
      playbackStore.setPlaying(false);
      playbackStore.setError(null);
    } catch (e) {
      console.error('Pause failed:', e);
      playbackStore.setError(String(e));
    }
  };

  const handleVolumeChange = async (deviceId: string, volume: number) => {
    playbackStore.setVolume(deviceId, volume);
    try {
      await invoke('set_volume', { deviceId, volume });
    } catch (e) {
      console.error('Volume change failed:', e);
    }
  };

  return (
    <div className="p-3 bg-zinc-900/80 backdrop-blur">
      <div className="flex items-center gap-3">
        {/* Playback Buttons */}
        <div className="flex items-center gap-1">
          {isPlaying ? (
            <button
              onClick={handlePause}
              disabled={!hasActiveDevices}
              className="p-2.5 rounded-full bg-zinc-800 text-zinc-200 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              title="Pause"
            >
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
              </svg>
            </button>
          ) : (
            <button
              onClick={handlePlay}
              disabled={!hasActiveDevices}
              className="p-2.5 rounded-full bg-purple-600 text-white hover:bg-purple-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              title="Play"
            >
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7z" />
              </svg>
            </button>
          )}
          <button
            onClick={handleStop}
            disabled={!isPlaying}
            className="p-2 rounded-full text-zinc-500 hover:text-zinc-300 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            title="Stop"
          >
            <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 6h12v12H6z" />
            </svg>
          </button>
        </div>

        {/* Active devices indicator */}
        <div className="flex-1 min-w-0">
          {error && (
            <p className="text-xs text-red-400 truncate mb-0.5" title={error}>{error}</p>
          )}
          {hasActiveDevices ? (
            <p className="text-xs text-zinc-500">
              {isPlaying ? (
                <span className="text-purple-400">Streaming</span>
              ) : (
                <span>Ready</span>
              )}
              {' to '}
              <span className="text-zinc-300">{activeDeviceIds.length} device{activeDeviceIds.length !== 1 ? 's' : ''}</span>
            </p>
          ) : (
            <p className="text-xs text-zinc-600">No device connected</p>
          )}
        </div>

        {/* Volume (master, shown when streaming) */}
        {hasActiveDevices && (
          <div className="flex items-center gap-2">
            <svg className="w-4 h-4 text-zinc-500" fill="currentColor" viewBox="0 0 24 24">
              <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0014 8.5v7a4.47 4.47 0 002.5-3.5z" />
            </svg>
            <input
              type="range"
              min="0"
              max="100"
              value={Math.round((volumes[activeDeviceIds[0]] ?? 0.5) * 100)}
              onChange={(e) => {
                const vol = parseInt(e.target.value) / 100;
                activeDeviceIds.forEach((id) => handleVolumeChange(id, vol));
              }}
              className="w-24 h-1 bg-zinc-700 rounded-full appearance-none cursor-pointer
                [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3
                [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-purple-500"
            />
          </div>
        )}
      </div>
    </div>
  );
}
