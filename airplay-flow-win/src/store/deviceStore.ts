import type { AirPlayDevice } from '../types/device';

type Listener = () => void;

let listeners: Listener[] = [];

const state = {
  devices: new Map<string, AirPlayDevice>(),
  devicesSnapshot: [] as AirPlayDevice[],
  scanning: false,
};

function notify() {
  listeners.forEach((l) => l());
}

export const deviceStore = {
  get devices(): AirPlayDevice[] {
    return state.devicesSnapshot;
  },

  get scanning(): boolean {
    return state.scanning;
  },

  getDevice(id: string): AirPlayDevice | undefined {
    return state.devices.get(id);
  },

  setDevice(device: AirPlayDevice) {
    state.devices.set(device.id, device);
    state.devicesSnapshot = Array.from(state.devices.values()).sort((a, b) =>
      a.name.localeCompare(b.name),
    );
    notify();
  },

  replaceDevices(devices: AirPlayDevice[]) {
    state.devices = new Map(devices.map((device) => [device.id, device]));
    state.devicesSnapshot = Array.from(state.devices.values()).sort((a, b) =>
      a.name.localeCompare(b.name),
    );
    notify();
  },

  removeDevice(id: string) {
    if (state.devices.delete(id)) {
      state.devicesSnapshot = Array.from(state.devices.values()).sort((a, b) =>
        a.name.localeCompare(b.name),
      );
      notify();
    }
  },

  setScanning(scanning: boolean) {
    if (state.scanning === scanning) return;
    state.scanning = scanning;
    notify();
  },

  subscribe(listener: Listener): () => void {
    listeners.push(listener);
    return () => {
      listeners = listeners.filter((l) => l !== listener);
    };
  },
};
