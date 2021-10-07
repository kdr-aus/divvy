use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

const O: Ordering = Ordering::Relaxed;

/// An atomic, thread shareable boolean switch.
pub struct Switch(Arc<AtomicBool>);

impl Switch {
    /// A new switch set to 'off' (`false`).
    pub fn off() -> Self {
        Switch(Arc::new(AtomicBool::new(false)))
    }

    /// Get the value of the switch.
    pub fn get(&self) -> bool {
        self.0.load(O)
    }

    /// Flip the switch to the 'on' (`true`) position.
    pub fn flip_on(&self) {
        self.0.store(true, O);
    }
}

impl Clone for Switch {
    fn clone(&self) -> Self {
        Switch(Arc::clone(&self.0))
    }
}
