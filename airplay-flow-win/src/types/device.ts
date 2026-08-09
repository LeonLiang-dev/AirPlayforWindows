export interface AirPlayDevice {
  id: string;
  name: string;
  host: string;
  port: number;
  protocol_version: string;
  features: number;
  flags: number;
  model: string;
  codecs: 'Pcm' | 'Alac' | 'Aac' | 'AlacAndAac' | 'Unknown';
  encryption: 'None' | 'Rsa' | 'FairPlay' | 'Unknown';
  requires_auth_setup: boolean;
  paired: boolean;
  connection_state:
    | 'Discovered'
    | 'Connecting'
    | 'Paired'
    | 'Ready'
    | 'Streaming'
    | { Error: string };
  public_key: string | null;
}

export interface AudioDeviceInfo {
  id: string;
  name: string;
  is_default: boolean;
  is_airplay_flow_virtual: boolean;
  sample_rate: number;
  channels: number;
}
