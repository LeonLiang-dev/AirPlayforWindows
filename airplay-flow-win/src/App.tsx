import { useState, useEffect, Component } from 'react';
import type { ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { deviceStore } from './store/deviceStore';
import { DeviceList } from './components/discovery/DeviceList';
import { PlaybackControls } from './components/player/PlaybackControls';
import { StatusBar } from './components/common/StatusBar';
import { AudioSettings } from './components/settings/AudioSettings';
import { playbackStore } from './hooks/usePlayback';
import type { AirPlayDevice } from './types/device';
import type { AudioDeviceInfo } from './types/device';

interface ConnectionStateChanged {
  device_id: string;
  state: AirPlayDevice['connection_state'];
}

// Error boundary to catch React render errors
class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { error: null };
  }
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    if (this.state.error) {
      return (
        <div style={{ color: '#d4d4d8', padding: 40, fontFamily: 'sans-serif', backgroundColor: '#09090b', minHeight: '100vh' }}>
          <h1 style={{ color: '#a855f7' }}>AirPlay Flow Win</h1>
          <h2 style={{ color: '#ef4444' }}>Application Error</h2>
          <pre style={{ fontSize: 14, whiteSpace: 'pre-wrap' }}>{this.state.error.message}</pre>
          <pre style={{ fontSize: 12, color: '#71717a', whiteSpace: 'pre-wrap' }}>{this.state.error.stack}</pre>
        </div>
      );
    }
    return this.props.children;
  }
}

export default function App() {
  const [activeTab, setActiveTab] = useState<'devices' | 'settings'>('devices');

  useEffect(() => {
    let cancelled = false;
    let unlisteners: (() => void)[] = [];

    const setup = async () => {
      try {
        const listeners = await Promise.all([
          listen<AirPlayDevice>('device-discovered', (event) => {
            deviceStore.setDevice(event.payload);
          }),
          listen<{ device_id: string }>('device-lost', (event) => {
            deviceStore.removeDevice(event.payload.device_id);
          }),
          listen<ConnectionStateChanged>('connection-state-changed', (event) => {
            const device = deviceStore.getDevice(event.payload.device_id);
            if (device) {
              deviceStore.setDevice({
                ...device,
                connection_state: event.payload.state,
              });
            }
          }),
          listen<{ device_ids: string[] }>('playback-started', () => {
            playbackStore.setError(null);
            playbackStore.setPlaying(true);
          }),
          listen<{ reason: string }>('playback-stopped', () => {
            playbackStore.setPlaying(false);
          }),
          listen<{ device_id: string; error: string }>('playback-error', (event) => {
            playbackStore.setError(event.payload.error);
          }),
          listen<AudioDeviceInfo>('audio-capture-changed', () => {
            playbackStore.setError(null);
          }),
        ]);

        if (cancelled) {
          listeners.forEach((unlisten) => unlisten());
          return;
        }
        unlisteners = listeners;

        const devices = await invoke<AirPlayDevice[]>('get_devices');
        if (!cancelled) deviceStore.replaceDevices(devices);

        await invoke('start_scan');
        if (!cancelled) deviceStore.setScanning(true);
      } catch (error) {
        console.error('App initialization failed:', error);
        if (!cancelled) deviceStore.setScanning(false);
      }
    };

    void setup();

    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
      unlisteners = [];
    };
  }, []);

  return (
    <ErrorBoundary>
      <div className="flex flex-col h-screen bg-zinc-950 text-zinc-100">
        {/* Header */}
        <header className="flex items-center justify-between px-4 py-3 border-b border-zinc-800">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-purple-500 to-blue-500 flex items-center justify-center">
              <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z" />
              </svg>
            </div>
            <h1 className="text-lg font-semibold tracking-tight">AirPlay Flow Win</h1>
          </div>
          <div className="flex gap-1 bg-zinc-800 rounded-lg p-0.5">
            <button
              onClick={() => setActiveTab('devices')}
              className={`px-3 py-1.5 text-sm rounded-md transition-colors ${
                activeTab === 'devices' ? 'bg-zinc-700 text-white' : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              Devices
            </button>
            <button
              onClick={() => setActiveTab('settings')}
              className={`px-3 py-1.5 text-sm rounded-md transition-colors ${
                activeTab === 'settings' ? 'bg-zinc-700 text-white' : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              Settings
            </button>
          </div>
        </header>

        <main className="flex-1 overflow-hidden">
          {activeTab === 'devices' && (
            <div className="flex flex-col h-full">
              <div className="flex-1 overflow-auto">
                <DeviceList />
              </div>
              <div className="border-t border-zinc-800">
                <PlaybackControls />
              </div>
            </div>
          )}
          {activeTab === 'settings' && (
            <AudioSettings />
          )}
        </main>

        <StatusBar />
      </div>
    </ErrorBoundary>
  );
}
