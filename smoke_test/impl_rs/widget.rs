// Toy widget implementation used by the covmap smoke test.

pub struct Widget {
    pub id: String,
    pub counter: i64,
}

impl Widget {
    pub fn new(id: impl Into<String>) -> Self {
        Widget { id: id.into(), counter: 0 }
    }

    pub fn increment(&mut self) {
        self.counter += 1;
    }

    pub fn decrement(&mut self) {
        self.counter -= 1;
    }

    pub fn reset(&mut self) {
        self.counter = 0;
    }

    pub fn status(&self) -> &'static str {
        if self.counter >= 0 { "ok" } else { "warn" }
    }
}
