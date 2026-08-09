use log::{debug, info, warn};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::airplay::device::{AirPlayDevice, CodecSupport, EncryptionType};
use crate::error::{AppError, AppResult};
use crate::events::{emit_device_discovered, emit_device_lost};

const SERVICE_TYPES: [&str; 2] = ["_raop._tcp.local.", "_airplay._tcp.local."];

/// mDNS discovery service that finds AirPlay and RAOP endpoints.
pub struct DiscoveryService {
    daemon: ServiceDaemon,
    /// Tracked services: full service name -> normalized device ID.
    tracked: Arc<Mutex<HashMap<String, String>>>,
    workers: Vec<JoinHandle<()>>,
}

impl DiscoveryService {
    pub fn new() -> AppResult<Self> {
        let daemon =
            ServiceDaemon::new().map_err(|error| AppError::DiscoveryError(error.to_string()))?;

        Ok(Self {
            daemon,
            tracked: Arc::new(Mutex::new(HashMap::new())),
            workers: Vec::new(),
        })
    }

    /// Start both browse streams and continuously process their events.
    pub fn start_browsing(
        &mut self,
        app_handle: AppHandle,
        registry: Arc<Mutex<HashMap<String, AirPlayDevice>>>,
    ) -> AppResult<()> {
        let mut receivers = Vec::with_capacity(SERVICE_TYPES.len());
        for service_type in SERVICE_TYPES {
            let receiver = self
                .daemon
                .browse(service_type)
                .map_err(|error| AppError::DiscoveryError(error.to_string()))?;
            receivers.push(receiver);
        }

        for receiver in receivers {
            let app_handle = app_handle.clone();
            let registry = registry.clone();
            let tracked = self.tracked.clone();

            self.workers.push(tokio::spawn(async move {
                while let Ok(event) = receiver.recv_async().await {
                    let should_stop = matches!(event, ServiceEvent::SearchStopped(_));
                    process_service_event(event, &app_handle, &registry, &tracked).await;
                    if should_stop {
                        break;
                    }
                }
            }));
        }

        info!("Started browsing for AirPlay devices");
        Ok(())
    }

    pub fn stop_browsing(&mut self) -> AppResult<()> {
        let mut first_error = None;

        for service_type in SERVICE_TYPES {
            if let Err(error) = self.daemon.stop_browse(service_type) {
                first_error.get_or_insert_with(|| error.to_string());
            }
        }

        for worker in self.workers.drain(..) {
            worker.abort();
        }

        info!("Stopped browsing for AirPlay devices");
        match first_error {
            Some(error) => Err(AppError::DiscoveryError(error)),
            None => Ok(()),
        }
    }
}

async fn process_service_event(
    event: ServiceEvent,
    app_handle: &AppHandle,
    registry: &Arc<Mutex<HashMap<String, AirPlayDevice>>>,
    tracked: &Arc<Mutex<HashMap<String, String>>>,
) {
    match event {
        ServiceEvent::SearchStarted(service_type) => {
            debug!("mDNS search started: {service_type}");
        }
        ServiceEvent::ServiceFound(_, fullname) => {
            debug!("mDNS service found: {fullname}");
        }
        ServiceEvent::ServiceResolved(info) => {
            let fullname = info.get_fullname().to_string();
            let Some(mut device) = parse_service_info(&info) else {
                debug!("Ignoring mDNS service without an IPv4 address: {fullname}");
                return;
            };

            let has_raop_endpoint = {
                let mut tracked = tracked.lock().await;
                tracked.insert(fullname.clone(), device.id.clone());
                tracked.iter().any(|(service, device_id)| {
                    device_id == &device.id && service.ends_with("._raop._tcp.local.")
                })
            };

            {
                let mut registry = registry.lock().await;
                if let Some(existing) = registry.get(&device.id) {
                    merge_existing_device(&mut device, existing, has_raop_endpoint, &fullname);
                }
                registry.insert(device.id.clone(), device.clone());
            }

            if let Err(error) = emit_device_discovered(app_handle, &device) {
                warn!("Unable to emit device discovery event: {error}");
            }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            let removed_id = {
                let mut tracked = tracked.lock().await;
                let removed_id = tracked.remove(&fullname);
                removed_id.and_then(|device_id| {
                    (!tracked.values().any(|id| id == &device_id)).then_some(device_id)
                })
            };

            if let Some(device_id) = removed_id {
                registry.lock().await.remove(&device_id);
                if let Err(error) = emit_device_lost(app_handle, &device_id) {
                    warn!("Unable to emit device removal event: {error}");
                }
            }
        }
        ServiceEvent::SearchStopped(service_type) => {
            debug!("mDNS search stopped: {service_type}");
        }
        _ => {}
    }
}

fn merge_existing_device(
    discovered: &mut AirPlayDevice,
    existing: &AirPlayDevice,
    has_raop_endpoint: bool,
    discovered_fullname: &str,
) {
    discovered.connection_state = existing.connection_state.clone();
    discovered.paired = existing.paired;

    if discovered.protocol_version.is_empty() {
        discovered.protocol_version = existing.protocol_version.clone();
    }
    if discovered.model.is_empty() {
        discovered.model = existing.model.clone();
    }
    if discovered.features == 0 {
        discovered.features = existing.features;
    }
    if discovered.flags == 0 {
        discovered.flags = existing.flags;
    }
    if discovered.codecs == CodecSupport::Unknown {
        discovered.codecs = existing.codecs.clone();
    }
    if discovered.encryption == EncryptionType::Unknown {
        discovered.encryption = existing.encryption.clone();
    }
    discovered.requires_auth_setup = discovered.requires_auth_setup || existing.requires_auth_setup;
    if discovered.public_key.is_none() {
        discovered.public_key = existing.public_key.clone();
    }

    // Audio must use the RAOP endpoint when both services describe one device.
    if has_raop_endpoint && !discovered_fullname.ends_with("._raop._tcp.local.") {
        discovered.host = existing.host.clone();
        discovered.port = existing.port;
    }
}

/// Parse an mDNS resolved service into an AirPlay device.
pub fn parse_service_info(info: &mdns_sd::ResolvedService) -> Option<AirPlayDevice> {
    let fullname = info.get_fullname();
    let instance_name = SERVICE_TYPES
        .iter()
        .find_map(|suffix| fullname.strip_suffix(suffix))
        .unwrap_or(fullname);
    let (raop_id, display_name) = instance_name
        .split_once('@')
        .map(|(id, name)| (Some(id), name))
        .unwrap_or((None, instance_name));

    let host: Ipv4Addr = info.get_addresses_v4().into_iter().next()?;
    let properties = info.get_properties();
    let raw_device_id = properties
        .get_property_val_str("deviceid")
        .or(raop_id)
        .unwrap_or(instance_name);
    let device_id = normalize_device_id(raw_device_id);

    let mut device = AirPlayDevice::new(device_id, display_name.to_string(), host, info.get_port());

    if let Some(version) = properties.get_property_val_str("fv") {
        device.protocol_version = version.to_string();
    }
    if let Some(features) = properties.get_property_val_str("features") {
        device.features = parse_bitmask(features);
    }
    if let Some(flags) = properties.get_property_val_str("flags") {
        device.flags = parse_bitmask(flags);
    }
    if let Some(model) = properties.get_property_val_str("am") {
        device.model = model.to_string();
    }

    if let Some(encryption) = properties.get_property_val_str("et") {
        let values = parse_number_list(encryption);
        (device.encryption, device.requires_auth_setup) = classify_encryption(&values);
    }

    if let Some(codecs) = properties
        .get_property_val_str("cn")
        .or_else(|| properties.get_property_val_str("md"))
    {
        let values = parse_number_list(codecs);
        let supports_alac = values.iter().any(|value| matches!(value, 1 | 3 | 4));
        let supports_aac = values.iter().any(|value| matches!(value, 2..=4));
        device.codecs = match (supports_alac, supports_aac) {
            (true, true) => CodecSupport::AlacAndAac,
            (true, false) => CodecSupport::Alac,
            (false, true) => CodecSupport::Aac,
            (false, false) if values.contains(&0) => CodecSupport::Pcm,
            _ => CodecSupport::Unknown,
        };
    }

    if let Some(public_key) = properties.get_property_val_str("pk") {
        device.public_key = Some(public_key.to_string());
    }

    Some(device)
}

fn normalize_device_id(value: &str) -> String {
    let normalized: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.is_empty() {
        value.to_lowercase()
    } else {
        normalized
    }
}

fn parse_bitmask(value: &str) -> u64 {
    value
        .split(',')
        .enumerate()
        .fold(0u64, |mask, (index, word)| {
            let word = word
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            let parsed = u64::from_str_radix(word, 16).unwrap_or(0);
            mask | parsed.checked_shl((index * 32) as u32).unwrap_or(0)
        })
}

fn parse_number_list(value: &str) -> Vec<u8> {
    value
        .split(',')
        .filter_map(|part| part.trim().parse::<u8>().ok())
        .collect()
}

fn classify_encryption(values: &[u8]) -> (EncryptionType, bool) {
    let requires_auth_setup = values.contains(&4);
    let encryption = if values.contains(&0) {
        EncryptionType::None
    } else if values.iter().any(|value| matches!(value, 3 | 4)) {
        EncryptionType::FairPlay
    } else if values.iter().any(|value| matches!(value, 1 | 2)) {
        EncryptionType::Rsa
    } else {
        EncryptionType::Unknown
    };
    (encryption, requires_auth_setup)
}

#[cfg(test)]
mod tests {
    use super::{classify_encryption, normalize_device_id, parse_bitmask, parse_number_list};
    use crate::airplay::device::EncryptionType;

    #[test]
    fn normalizes_device_ids_across_airplay_and_raop_records() {
        assert_eq!(normalize_device_id("AA:BB:CC:DD:EE:FF"), "aabbccddeeff");
        assert_eq!(normalize_device_id("AABBCCDDEEFF"), "aabbccddeeff");
    }

    #[test]
    fn parses_single_and_split_feature_bitmasks() {
        assert_eq!(parse_bitmask("0x5A7FFFF7"), 0x5A7FFFF7);
        assert_eq!(parse_bitmask("0x00000001,0x00000002"), 0x0000000200000001);
    }

    #[test]
    fn parses_comma_separated_capabilities() {
        assert_eq!(parse_number_list("0,1, 2"), vec![0, 1, 2]);
    }

    #[test]
    fn treats_edifier_et_zero_four_as_clear_audio_with_auth_setup() {
        assert_eq!(classify_encryption(&[0, 4]), (EncryptionType::None, true));
    }
}
