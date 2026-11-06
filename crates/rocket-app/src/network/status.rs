//! The status shown in the Network panel and in Settings.

use rocket_network::NodeProfile;

use super::service::NetworkService;

/// Shown when the device key cannot be read. There is no key-file fallback.
pub(super) const KEYCHAIN_OFF_REASON: &str = "The keychain is not available, so networking is off.";
/// Shown when the TLS setup fails for another reason.
pub(super) const TRANSPORT_OFF_REASON: &str = "Networking could not start on this computer.";
/// Shown before `start` has run.
pub(super) const NOT_STARTED_REASON: &str = "Networking is starting.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkState {
    Running,
    Off { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkStatus {
    pub state: NetworkState,
    pub port: Option<u16>,
    pub listener_available: bool,
    pub discovery_available: bool,
    pub hidden: bool,
    pub profile: NodeProfile,
    pub device_short_id: Option<String>,
    pub intro_seen: bool,
    pub warnings: Vec<String>,
}

/// The user text for a file that was set aside as damaged. `file` is the file name the
/// repository reports, possibly with the `.corrupt-<secs>` suffix.
pub(super) fn warning_for(file: &str) -> String {
    let name = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file);
    let name = name.split(".corrupt-").next().unwrap_or(name);
    if name == "blocked.yml" {
        "Your blocked devices list was damaged and has been reset.".to_string()
    } else {
        format!("{name} was damaged and has been reset.")
    }
}

impl NetworkService {
    pub fn status(&self) -> NetworkStatus {
        let st = self.lock();
        NetworkStatus {
            state: st.net.clone(),
            port: st.port,
            listener_available: st.listener_available,
            discovery_available: st.discovery_available,
            hidden: st.settings.hidden,
            profile: st.profile.clone(),
            device_short_id: st.runtime.as_ref().map(|r| r.local.device_id.short()),
            intro_seen: st.settings.intro_seen,
            warnings: st.warnings.clone(),
        }
    }
}
