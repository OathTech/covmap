pub struct Queue { buf: Vec<Task>, cap: usize }

impl Queue {
    pub fn enqueue(&mut self, t: Task) -> Result<(), QueueFull> {
        if self.buf.len() == self.cap { return Err(QueueFull); }
        self.buf.push(t);
        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<Task> {
        if self.buf.is_empty() { None } else { Some(self.buf.remove(0)) }
    }

    pub fn size(&self) -> usize { self.buf.len() }
}
