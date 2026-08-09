import { useSyncExternalStore } from 'react';
import { deviceStore } from '../../store/deviceStore';
import { playbackStore } from '../../hooks/usePlayback';

export function StatusBar() {
  const devices = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.devices,
  );
  const { isPlaying } = useSyncExternalStore(
    playbackStore.subscribe,
    () => playbackStore.snapshot,
  );

  const connectedCount = devices.filter(
    (d) =>
      typeof d.connection_state === 'string' &&
      ['Ready', 'Streaming', 'Paired'].includes(d.connection_state),
  ).length;

  return (
    <footer className="flex items-center justify-between px-4 py-1.5 border-t border-zinc-800 bg-zinc-900/50 text-xs text-zinc-500">
      <div className="flex items-center gap-4">
        <span>
          {devices.length} discovered
        </span>
        <span className="text-zinc-700">|</span>
        <span>
          {connectedCount} connected
        </span>
        {isPlaying && (
          <>
            <span className="text-zinc-700">|</span>
            <span className="text-purple-400 flex items-center gap-1">
              <span className="w-1.5 h-1.5 rounded-full bg-purple-400 animate-pulse" />
              Streaming
            </span>
          </>
        )}
      </div>
      <span>AirPlay Flow Win v0.1.0</span>
    </footer>
  );
}
