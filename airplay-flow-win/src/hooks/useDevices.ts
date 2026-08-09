import { useSyncExternalStore } from 'react';
import { deviceStore } from '../store/deviceStore';

/**
 * React hook to subscribe to device store changes.
 * Uses useSyncExternalStore for React 18+ concurrent mode safety.
 */
export function useDevices() {
  const devices = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.devices,
  );
  const scanning = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.scanning,
  );
  return { devices, scanning };
}

/**
 * Hook for device connection state tracking
 */
export function useDeviceConnection(deviceId: string) {
  const device = useSyncExternalStore(
    deviceStore.subscribe,
    () => deviceStore.getDevice(deviceId),
  );
  return device?.connection_state ?? null;
}
