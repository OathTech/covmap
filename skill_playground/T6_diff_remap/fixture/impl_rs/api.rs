pub fn create(body: &Json) -> Id {
    store().insert(Item::from(body))
}

pub fn read(id: Id) -> Option<Item> {
    store().get(id)
}

pub fn delete(id: Id) -> bool {
    store().remove(id)
}
