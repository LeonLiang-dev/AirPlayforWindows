import { useSyncExternalStore } from 'react';

type Listener = () => void;
let listeners: Listener[] = [];

interface PlaybackState {
  isPlaying: boolean;
  activeDeviceIds: string[];
  volumes: Record<string, number>;
  error: string | null;
}

let state: PlaybackState = {
  isPlaying: false,
  activeDeviceIds: [],
  volumes: {},
  error: null,
};

function notify() {
  listeners.forEach((l) => l());
}

export const playbackStore = {
  get snapshot() { return state; },
  get isPlaying() { return state.isPlaying; },
  get activeDeviceIds() { return state.activeDeviceIds; },
  get volumes() { return state.volumes; },
  get error() { return state.error; },

  setPlaying(playing: boolean) {
    if (state.isPlaying === playing) return;
    state = { ...state, isPlaying: playing };
    notify();
  },

  setError(error: string | null) {
    if (state.error === error) return;
    state = { ...state, error };
    notify();
  },

  setActiveDevices(ids: string[]) {
    const activeDeviceIds = Array.from(new Set(ids));
    if (
      activeDeviceIds.length === state.activeDeviceIds.length &&
      activeDeviceIds.every((id, index) => id === state.activeDeviceIds[index])
    ) return;
    state = { ...state, activeDeviceIds };
    notify();
  },

  setVolume(deviceId: string, volume: number) {
    const nextVolume = Math.max(0, Math.min(1, volume));
    if (state.volumes[deviceId] === nextVolume) return;
    state = {
      ...state,
      volumes: { ...state.volumes, [deviceId]: nextVolume },
    };
    notify();
  },

  subscribe(l: Listener): () => void {
    listeners.push(l);
    return () => { listeners = listeners.filter((x) => x !== l); };
  },
};

export function usePlayback() {
  return useSyncExternalStore(playbackStore.subscribe, () => playbackStore.snapshot);
}
