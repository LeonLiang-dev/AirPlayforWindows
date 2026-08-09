import { useSyncExternalStore } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { deviceStore } from '../../store/deviceStore';
import { DeviceCard } from './DeviceCard';

export function DeviceList() {
  const devices = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.devices,
  );
  const scanning = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.scanning,
  );

  if (devices.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 p-8">
        <div className={`w-12 h-12 rounded-full bg-zinc-800 flex items-center justify-center ${scanning ? 'animate-pulse' : ''}`}>
          <svg className="w-6 h-6 text-zinc-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M19.114 5.636a9 9 0 010 12.728M16.463 8.288a5.25 5.25 0 010 7.424M6.75 8.25l4.72-4.72a.75.75 0 011.28.53v15.88a.75.75 0 01-1.28.53l-4.72-4.72H4.51c-.88 0-1.704-.507-1.938-1.354A9.009 9.009 0 012.25 12c0-.83.112-1.633.322-2.396C2.806 8.756 3.63 8.25 4.51 8.25H6.75z"
            />
          </svg>
        </div>
        <p className="text-zinc-500 text-sm">
          {scanning ? 'Scanning for AirPlay devices...' : 'No devices found'}
        </p>
        <RefreshButton />
      </div>
    );
  }

  return (
    <div className="p-3 space-y-2">
      <div className="flex items-center justify-between mb-3 px-1">
        <span className="text-xs text-zinc-500 font-medium uppercase tracking-wider">
          {devices.length} device{devices.length !== 1 ? 's' : ''} found
        </span>
        <RefreshButton />
      </div>
      {devices.map((device) => (
        <DeviceCard key={device.id} device={device} />
      ))}
    </div>
  );
}

function RefreshButton() {
  const scanning = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.scanning,
  );

  return (
    <button
      onClick={async () => {
        try {
          if (scanning) {
            await invoke('stop_scan');
            deviceStore.setScanning(false);
          } else {
            await invoke('start_scan');
            deviceStore.setScanning(true);
          }
        } catch (error) {
          console.error('Unable to change discovery state:', error);
          deviceStore.setScanning(false);
        }
      }}
      className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md border transition-colors ${
        scanning
          ? 'border-purple-500/30 text-purple-400 bg-purple-500/10'
          : 'border-zinc-700 text-zinc-400 hover:text-zinc-200 hover:border-zinc-600'
      }`}
    >
      <svg
        className={`w-3.5 h-3.5 ${scanning ? 'animate-spin' : ''}`}
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={2}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182"
        />
      </svg>
      {scanning ? 'Stop scan' : 'Scan'}
    </button>
  );
}
