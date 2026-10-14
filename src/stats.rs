use std::time::Instant;

/// Tracks overall transfer progress.
pub struct TransferStats {
    pub total_bytes: u64,
    pub downloaded: u64,
    start: Instant,
}

impl TransferStats {
    /// Creates a tracker for a total byte count.
    pub fn new(total_bytes: u64) -> Self {
        Self {
            total_bytes,
            downloaded: 0,
            start: Instant::now(),
        }
    }

    /// Adds downloaded bytes to the running total.
    pub fn update(&mut self, bytes: u64) {
        self.downloaded += bytes;
    }

    /// Returns overall throughput in bytes per second.
    pub fn speed(&self) -> u64 {
        let secs = self.start.elapsed().as_secs().max(1);
        self.downloaded / secs
    }

    /// Returns the remaining bytes to download.
    pub fn left(&self) -> u64 {
        self.total_bytes - self.downloaded
    }

    /// Estimates the remaining time in seconds, if the speed is known.
    pub fn eta(&self) -> Option<u64> {
        let speed = self.speed();
        if speed > 0 {
            let remaining = self.total_bytes.saturating_sub(self.downloaded);
            Some(remaining / speed)
        } else {
            None
        }
    }

    /// Returns the completion fraction in the range `0.0..=1.0`.
    pub fn progress(&self) -> f64 {
        self.downloaded as f64 / self.total_bytes as f64
    }
}
