import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AudioDeviceInfo } from '../../types/device';

export function AudioSettings() {
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    setLoading(true);
    setError(null);
    try {
      setDevices(await invoke<AudioDeviceInfo[]>('get_audio_devices'));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const defaultDevice = devices.find((device) => device.is_default);
  const virtualDevice = devices.find((device) => device.is_airplay_flow_virtual);

  return (
    <div className="p-5 max-w-2xl">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="text-sm font-medium text-zinc-300">Windows audio capture</h2>
          <p className="text-sm text-zinc-500 mt-1">
            AirPlay Flow Win follows the default output selected in Windows automatically.
          </p>
        </div>
        <button
          onClick={() => void refresh()}
          disabled={loading}
          className="px-3 py-1.5 text-xs rounded-lg border border-zinc-700 text-zinc-300 hover:bg-zinc-800 disabled:opacity-40"
        >
          {loading ? 'Checking…' : 'Refresh'}
        </button>
      </div>

      <div className="mt-5 rounded-xl border border-zinc-800 bg-zinc-900/50 p-4">
        {error ? (
          <p className="text-sm text-red-400">{error}</p>
        ) : defaultDevice ? (
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-lg bg-purple-500/15 text-purple-400 flex items-center justify-center">
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 8.25l4.72-4.72a.75.75 0 011.28.53v15.88a.75.75 0 01-1.28.53l-4.72-4.72H4.5A2.25 2.25 0 012.25 13.5v-3A2.25 2.25 0 014.5 8.25h2.25zM16.5 8.25a5.25 5.25 0 010 7.5" />
              </svg>
            </div>
            <div className="min-w-0">
              <p className="text-sm text-zinc-200 truncate">{defaultDevice.name}</p>
              <p className="text-xs text-zinc-500 mt-0.5">
                Current Windows default · {defaultDevice.sample_rate || '—'} Hz · {defaultDevice.channels || '—'} channels
              </p>
              <p className={`text-xs mt-1 ${defaultDevice.is_airplay_flow_virtual ? 'text-emerald-400' : 'text-amber-400'}`}>
                {defaultDevice.is_airplay_flow_virtual
                  ? 'Virtual output active · audio is sent to AirPlay without local playback'
                  : virtualDevice
                    ? 'Physical output active · select AirPlay Flow Win in Windows to stop double playback'
                    : 'Physical output active · Windows and AirPlay will both play'}
              </p>
            </div>
          </div>
        ) : (
          <p className="text-sm text-zinc-500">
            {loading ? 'Reading Windows audio devices…' : 'No active Windows output device found.'}
          </p>
        )}
      </div>

      <p className="text-xs text-zinc-600 mt-3">
        During streaming, switching the Windows default output restarts capture automatically.
      </p>
    </div>
  );
}
