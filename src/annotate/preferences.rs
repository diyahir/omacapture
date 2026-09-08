use crate::app::Grabbit;
use std::rc::Rc;

pub fn open(gb: &Rc<Grabbit>) {
    let _ = gb;
    tracing::warn!("preferences not implemented yet");
}
