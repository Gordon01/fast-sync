//! WFX paths with numeric device addresses and storage-relative device paths.
#![forbid(unsafe_code)]

use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

/// WFT port used when a device name contains only an IP address.
pub const DEFAULT_PORT: u16 = 1234;

/// A listing target: the plugin root or a path on a device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemotePath {
    /// The plugin directory containing device entries.
    Root,
    /// A directory on the addressed device.
    Device(DevicePath),
}

/// A device address and its unmodified WFX-relative path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicePath {
    /// Numeric device address, including the WFT port.
    pub address: SocketAddr,
    /// Slash-separated path preserved exactly as supplied by Double Commander.
    pub path: String,
}

/// A remote name that does not follow the supported WFX syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidPath(String);

/// Reports the rejected remote name.
impl fmt::Display for InvalidPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid WFX path {:?}", self.0)
    }
}

impl std::error::Error for InvalidPath {}

/// Recognizes the plugin root or delegates to device-path validation.
impl FromStr for RemotePath {
    type Err = InvalidPath;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "/" => Ok(Self::Root),
            _ => value.parse().map(Self::Device),
        }
    }
}

/// Splits a slash-prefixed device name from its unmodified relative path.
impl FromStr for DevicePath {
    type Err = InvalidPath;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidPath(value.to_owned());
        let path = value.strip_prefix('/').ok_or_else(invalid)?;
        let (device, path) = path.split_once('/').unwrap_or((path, ""));
        let address = device
            .parse::<SocketAddr>()
            .or_else(|_| {
                device
                    .parse::<IpAddr>()
                    .map(|ip| SocketAddr::new(ip, DEFAULT_PORT))
            })
            .map_err(|_| invalid())?;
        Ok(Self {
            address,
            path: path.to_owned(),
        })
    }
}

impl DevicePath {
    /// Parses a file name, rejecting plugin/device roots and trailing slashes.
    pub fn parse_file(value: &str) -> Result<Self, InvalidPath> {
        let remote: Self = value.parse()?;
        if remote.path.is_empty() || value.ends_with('/') {
            return Err(InvalidPath(value.to_owned()));
        }
        Ok(remote)
    }
}

/// Formats a device entry name, omitting the default WFT port.
pub fn device_name(address: &SocketAddr) -> String {
    if address.port() == DEFAULT_PORT {
        address.ip().to_string()
    } else {
        address.to_string()
    }
}

/// Formats a WFX name while preserving its device-relative path.
impl fmt::Display for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root => f.write_str("/"),
            Self::Device(DevicePath { address, path }) => {
                write!(f, "/{}", device_name(address))?;
                if !path.is_empty() {
                    write!(f, "/{path}")?;
                }
                Ok(())
            }
        }
    }
}
