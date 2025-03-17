use std::time::Instant;

// Accumulates overall transfer statistics.
pub struct TransferStats {
    pub total_bytes: u64,
    pub downloaded: u64,
    start: Instant,
}

impl TransferStats {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            total_bytes,
            downloaded: 0,
            start: Instant::now(),
        }
    }

    pub fn update(&mut self, bytes: u64) {
        self.downloaded += bytes;
    }

    /// Returns overall speed in bytes per second.
    pub fn speed(&self) -> u64 {
        let secs = self.start.elapsed().as_secs().max(1);
        self.downloaded / secs
    }

    pub fn left(&self) -> u64 {
        self.total_bytes - self.downloaded
    }

    /// Estimated seconds remaining for overall transfer.
    pub fn eta(&self) -> Option<u64> {
        let speed = self.speed();
        if speed > 0 {
            let remaining = self.total_bytes.saturating_sub(self.downloaded);
            Some(remaining / speed)
        } else {
            None
        }
    }

    /// Returns the progress as a fraction between 0.0 and 1.0.
    pub fn progress(&self) -> f64 {
        self.downloaded as f64 / self.total_bytes as f64
    }
}
