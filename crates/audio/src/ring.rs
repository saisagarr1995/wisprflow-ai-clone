use std::collections::VecDeque;

/// Rolling history of the most recent audio, in 16 kHz mono f32 samples.
///
/// A single writer appends; readers snapshot ranges by absolute sample index.
/// Absolute indices (`total_written`) never wrap, so a session marker taken at
/// key-down stays valid even after the buffer discards old audio.
#[derive(Debug)]
pub struct RollingBuffer {
    samples: VecDeque<f32>,
    capacity: usize,
    /// Total samples ever written; index of the *next* sample to be written.
    total_written: u64,
}

impl RollingBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            total_written: 0,
        }
    }

    pub fn push(&mut self, chunk: &[f32]) {
        for &s in chunk {
            if self.samples.len() == self.capacity {
                self.samples.pop_front();
            }
            self.samples.push_back(s);
        }
        self.total_written += chunk.len() as u64;
    }

    /// Absolute index of the next sample to be written.
    pub fn head(&self) -> u64 {
        self.total_written
    }

    /// Absolute index of the oldest sample still retained.
    pub fn tail(&self) -> u64 {
        self.total_written - self.samples.len() as u64
    }

    /// Copy `[from, to)` by absolute index. The range is clamped to what is
    /// still retained; audio that has already been overwritten is silently
    /// dropped from the front (callers ask for pre-roll opportunistically).
    pub fn snapshot(&self, from: u64, to: u64) -> Vec<f32> {
        let from = from.max(self.tail());
        let to = to.min(self.head());
        if from >= to {
            return Vec::new();
        }
        let start = (from - self.tail()) as usize;
        let len = (to - from) as usize;
        self.samples.iter().skip(start).take(len).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_returns_pushed_samples() {
        let mut rb = RollingBuffer::new(100);
        rb.push(&[1.0, 2.0, 3.0]);
        assert_eq!(rb.snapshot(0, 3), vec![1.0, 2.0, 3.0]);
        assert_eq!(rb.snapshot(1, 2), vec![2.0]);
    }

    #[test]
    fn wraparound_discards_oldest_but_keeps_absolute_indices() {
        let mut rb = RollingBuffer::new(4);
        rb.push(&[0.0, 1.0, 2.0, 3.0]);
        rb.push(&[4.0, 5.0]); // evicts 0.0, 1.0
        assert_eq!(rb.head(), 6);
        assert_eq!(rb.tail(), 2);
        // Request includes evicted range; clamped to what survives.
        assert_eq!(rb.snapshot(0, 6), vec![2.0, 3.0, 4.0, 5.0]);
        assert_eq!(rb.snapshot(3, 5), vec![3.0, 4.0]);
    }

    #[test]
    fn empty_or_inverted_ranges_yield_empty() {
        let mut rb = RollingBuffer::new(4);
        rb.push(&[1.0, 2.0]);
        assert!(rb.snapshot(2, 2).is_empty());
        assert!(rb.snapshot(5, 9).is_empty());
    }
}
