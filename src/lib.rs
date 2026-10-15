//! Client library for the WiFi File Transfer Android app HTTP API.
//!
//! The main entry point is [`wft::Wft`]. Use it to query device information,
//! list storage directories, and download files.
//!
//! ```
//! use std::net::SocketAddr;
//!
//! use fast_sync::wft::{Result, Wft};
//!
//! async fn connect(address: SocketAddr) -> Result<Wft> {
//!     Wft::new(address).await
//! }
//! ```
pub mod wft;
