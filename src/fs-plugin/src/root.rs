#![forbid(unsafe_code)]

use std::net::SocketAddr;

use crate::remote_path::device_name;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
}

pub fn root_entries(address: &SocketAddr) -> Vec<Entry> {
    vec![Entry {
        name: device_name(address),
        is_directory: true,
        size: 0,
    }]
}
