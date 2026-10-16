use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
}

pub fn device_name(address: &SocketAddr) -> String {
    address.ip().to_string()
}

pub fn root_entries(address: &SocketAddr) -> Vec<Entry> {
    vec![Entry {
        name: device_name(address),
        is_directory: true,
        size: 0,
    }]
}

pub fn normalize_path(path: &str) -> Result<String, String> {
    let path = path.replace('\\', "/");
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Ok(String::new());
    }
    if path
        .split('/')
        .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(format!("invalid WFX path {path:?}"));
    }
    Ok(path.to_owned())
}

pub fn device_relative_path(path: &str, address: &SocketAddr) -> Result<Option<String>, String> {
    let path = normalize_path(path)?;
    if path.is_empty() {
        return Ok(None);
    }

    let (device, relative_path) = path
        .split_once('/')
        .map_or((path.as_str(), ""), |(device, relative)| (device, relative));
    if device != device_name(address) {
        return Err(format!("unknown device in WFX path {path:?}"));
    }
    Ok(Some(relative_path.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{device_name, device_relative_path, normalize_path};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn device_address() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)), 1234)
    }

    #[test]
    fn normalizes_wfx_separators_without_changing_case() {
        assert_eq!(
            normalize_path("\\Pictures\\Camera\\").unwrap(),
            "Pictures/Camera"
        );
        assert_eq!(normalize_path("/").unwrap(), "");
    }

    #[test]
    fn rejects_invalid_path_components() {
        assert!(normalize_path("../private").is_err());
        assert!(normalize_path("Pictures//Camera").is_err());
    }

    #[test]
    fn keeps_device_prefix_out_of_remote_relative_paths() {
        let address = device_address();
        let device = device_name(&address);
        assert_eq!(
            device_relative_path(&format!("\\{device}\\DCIM\\Camera\\"), &address).unwrap(),
            Some("DCIM/Camera".to_owned())
        );
        assert_eq!(
            device_relative_path(&device, &address).unwrap(),
            Some(String::new())
        );
        assert!(device_relative_path("Other device/DCIM", &address).is_err());
    }

    #[test]
    fn lists_the_configured_device_at_the_plugin_root() {
        let address = device_address();
        assert_eq!(device_relative_path("\\", &address).unwrap(), None);
    }
}
