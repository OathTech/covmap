pub struct Stack {
    items: Vec<i64>,
}

impl Stack {
    pub fn new() -> Self { Stack { items: Vec::new() } }
    pub fn push(&mut self, x: i64) { self.items.push(x); }
    pub fn pop(&mut self) -> Option<i64> { self.items.pop() }
    pub fn peek(&self) -> Option<&i64> { self.items.last() }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
}
