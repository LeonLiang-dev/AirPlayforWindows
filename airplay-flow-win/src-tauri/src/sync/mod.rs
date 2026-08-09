/// Multi-room sync engine stub.
///
/// Phase 2 will implement NTP clock sync and RTP timestamp coordination.
/// For MVP, this provides the interface but doesn't synchronize.
pub mod engine;
pub use engine::SyncEngine;
