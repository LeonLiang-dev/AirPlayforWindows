// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(debug_assertions)]
    if let Ok(endpoint) = std::env::var("AIRPLAY_DIAGNOSTIC_RECEIVER") {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .try_init();
        if let Err(error) = run_receiver_diagnostic(&endpoint) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    app_lib::run();
}

#[cfg(debug_assertions)]
fn run_receiver_diagnostic(endpoint: &str) -> Result<(), String> {
    use app_lib::airplay::device::AirPlayDevice;
    use app_lib::airplay::session::{RtspSession, SessionState};
    use std::net::Ipv4Addr;

    let (host, port) = endpoint
        .split_once(':')
        .map(|(host, port)| {
            port.parse::<u16>()
                .map(|port| (host, port))
                .map_err(|error| format!("Invalid diagnostic port: {error}"))
        })
        .transpose()?
        .unwrap_or((endpoint, 7000));
    let host = host
        .parse::<Ipv4Addr>()
        .map_err(|error| format!("Invalid diagnostic address: {error}"))?;
    let mut device = AirPlayDevice::new(
        "diagnostic".to_string(),
        "Diagnostic receiver".to_string(),
        host,
        port,
    );
    device.requires_auth_setup = true;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Unable to start diagnostic runtime: {error}"))?;
    runtime.block_on(async move {
        let mut session = RtspSession::new(&device);
        session.connect().await.map_err(|error| error.to_string())?;
        if session.state != SessionState::SetupComplete {
            return Err(format!("Unexpected session state: {:?}", session.state));
        }
        session
            .teardown()
            .await
            .map_err(|error| error.to_string())?;
        println!("RAOP diagnostic connection succeeded");
        Ok(())
    })
}
