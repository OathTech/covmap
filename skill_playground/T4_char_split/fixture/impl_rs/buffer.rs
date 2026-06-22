pub struct RingBuffer {
    buf: Vec<u8>,
    mask: usize,
    head: usize,
    tail: usize,
}

impl RingBuffer {
    /// `cap` must be a power of two; we mask indices rather than take a
    /// modulus on the hot path.
    pub fn with_capacity(cap: usize) -> Self {
        assert!(cap.is_power_of_two(), "capacity must be a power of two");
        RingBuffer {
            buf: vec![0; cap],
            mask: cap - 1,
            head: 0,
            tail: 0,
        }
    }

    pub fn push(&mut self, b: u8) {
        self.buf[self.head & self.mask] = b;
        self.head += 1;
    }
}
