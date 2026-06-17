use std::collections::VecDeque;

pub struct Queue {
    items: VecDeque<i64>,
}

impl Queue {
    pub fn new() -> Self { Queue { items: VecDeque::new() } }
    pub fn enqueue(&mut self, x: i64) { self.items.push_back(x); }
    pub fn dequeue(&mut self) -> Option<i64> { self.items.pop_front() }
    pub fn len(&self) -> usize { self.items.len() }
}
