use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

pub struct CtxMap {
  items: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}
impl CtxMap {
  pub fn new() -> Self { Self { items: HashMap::new() } }
  pub fn set<T: Send + Sync + 'static>(&mut self, ctx: T) {
    let prev = self.items.insert(ctx.type_id(), Box::new(ctx));
    assert!(prev.is_none(), "Context cannot be reassigned")
  }
  pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
    let val = self.items.get(&TypeId::of::<T>())?;
    Some(val.downcast_ref().expect("keyed with TypeId"))
  }
  pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
    let val = self.items.get_mut(&TypeId::of::<T>())?;
    Some(val.downcast_mut().expect("keyed with TypeId"))
  }
}
