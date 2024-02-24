use std::any::{Any, TypeId};
use std::collections::HashMap;

use trait_set::trait_set;

trait_set! {
  pub trait Ctx = Send + Sync + 'static
}

pub struct CtxMap {
  items: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}
impl CtxMap {
  pub fn new() -> Self { Self { items: HashMap::new() } }
  pub fn set<T: Ctx>(&mut self, ctx: T) { self.items.insert(ctx.type_id(), Box::new(ctx)); }
  pub fn get<T: Ctx>(&self) -> Option<&T> {
    let val = self.items.get(&TypeId::of::<T>())?;
    Some(val.downcast_ref().expect("keyed with TypeId"))
  }
  pub fn get_mut<T: Ctx>(&mut self) -> Option<&mut T> {
    let val = self.items.get_mut(&TypeId::of::<T>())?;
    Some(val.downcast_mut().expect("keyed with TypeId"))
  }
}
