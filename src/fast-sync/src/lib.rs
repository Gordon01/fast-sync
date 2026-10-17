//! Async client for the WiFi File Transfer Android app HTTP API.
//!
//! The main entry point is [`wft::Wft`]. Connect to a device, inspect its
//! storage, list directories, and download files. See the [`wft`] module for
//! method behavior and error details.
//!
//! ```
//! use std::net::SocketAddr;
//!
//! use fast_sync::wft::Wft;
//!
//! async fn list_camera(address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
//!     let wft = Wft::new(address).await?;
//!     let entries = wft.directory("DCIM/Camera").await?;
//!     for entry in entries {
//!         println!("{entry:?}");
//!     }
//!     Ok(())
//! }
//! ```
pub mod wft;
