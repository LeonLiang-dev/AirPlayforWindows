import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AirPlayDevice } from '../../types/device';
import { playbackStore } from '../../hooks/usePlayback';

interface DeviceCardProps {
  device: AirPlayDevice;
}

export function DeviceCard({ device }: DeviceCardProps) {
  const [connecting, setConnecting] = useState(false);

  const isConnected =
    typeof device.connection_state === 'string' &&
    ['Ready', 'Streaming', 'Paired'].includes(device.connection_state);

  const isStreaming = device.connection_state === 'Streaming';

  const stateLabel = getStateLabel(device.connection_state);
  const stateColor = getStateColor(device.connection_state);

  const handleConnect = async () => {
    setConnecting(true);
    try {
      await invoke('connect_device', { deviceId: device.id });
      playbackStore.setActiveDevices([
        ...playbackStore.activeDeviceIds,
        device.id,
      ]);
    } catch (e) {
      console.error('Connection failed:', e);
    } finally {
      setConnecting(false);
    }
  };

  const handleDisconnect = async () => {
    try {
      await invoke('disconnect_device', { deviceId: device.id });
      playbackStore.setActiveDevices(
        playbackStore.activeDeviceIds.filter((id) => id !== device.id),
      );
    } catch (e) {
      console.error('Disconnect failed:', e);
    }
  };

  return (
    <div
      className={`group flex items-center gap-3 p-3 rounded-xl border transition-all ${
        isStreaming
          ? 'border-purple-500/40 bg-purple-500/5'
          : isConnected
            ? 'border-zinc-700/50 bg-zinc-800/30'
            : 'border-zinc-800 bg-zinc-900/50 hover:border-zinc-700'
      }`}
    >
      {/* Device Icon */}
      <div
        className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 ${
          isStreaming
            ? 'bg-purple-500/20 text-purple-400'
            : isConnected
              ? 'bg-green-500/20 text-green-400'
              : 'bg-zinc-800 text-zinc-500'
        }`}
      >
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19.114 5.636a9 9 0 010 12.728M16.463 8.288a5.25 5.25 0 010 7.424M6.75 8.25l4.72-4.72a.75.75 0 011.28.53v15.88a.75.75 0 01-1.28.53l-4.72-4.72H4.51c-.88 0-1.704-.507-1.938-1.354A9.009 9.009 0 012.25 12c0-.83.112-1.633.322-2.396C2.806 8.756 3.63 8.25 4.51 8.25H6.75z"
          />
        </svg>
      </div>

      {/* Device Info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-zinc-100 truncate">{device.name}</h3>
          <span className={`inline-block w-1.5 h-1.5 rounded-full ${stateColor}`} />
        </div>
        <p className="text-xs text-zinc-500 truncate">
          {device.model || device.host} — {stateLabel}
        </p>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-1.5">
        {isConnected ? (
          <button
            onClick={handleDisconnect}
            className="px-3 py-1.5 text-xs rounded-lg border border-red-500/30 text-red-400 hover:bg-red-500/10 hover:border-red-500/50 transition-colors"
          >
            Disconnect
          </button>
        ) : (
          <button
            onClick={handleConnect}
            disabled={connecting}
            className={`px-3 py-1.5 text-xs rounded-lg border transition-colors ${
              connecting
                ? 'border-zinc-700 text-zinc-600 cursor-wait'
                : 'border-zinc-700 text-zinc-300 hover:bg-zinc-800 hover:border-zinc-600'
            }`}
          >
            {connecting ? '...' : 'Connect'}
          </button>
        )}
      </div>
    </div>
  );
}

function getStateLabel(state: AirPlayDevice['connection_state']): string {
  if (typeof state === 'object' && 'Error' in state) return `Error: ${state.Error}`;
  return state as string;
}

function getStateColor(state: AirPlayDevice['connection_state']): string {
  if (typeof state === 'object' && 'Error' in state) return 'bg-red-500';
  switch (state) {
    case 'Streaming':
      return 'bg-purple-400 animate-pulse';
    case 'Ready':
    case 'Paired':
      return 'bg-green-500';
    case 'Connecting':
      return 'bg-yellow-400 animate-pulse';
    default:
      return 'bg-zinc-600';
  }
}
