//! Progress items.
use crate::Switch;
use crossbeam_channel::*;
use parking_lot::Mutex;
use std::{borrow::Cow, fmt, sync::Arc};

// ###### PROGRESS #############################################################
/// An in-progress report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// A progress message, if any.
    pub msg: Cow<'static, str>,
    /// A percentage of progress (if known). Should be an integer from 0-100, but there is no
    /// guarantee, any `u8` value is potential.
    pub pct: u8,
}

impl fmt::Display for Progress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.msg.is_empty() {
            write!(f, "{:>3}%", self.pct)
        } else {
            write!(f, "{:>3}% - {}", self.pct, self.msg)
        }
    }
}

// ###### PROGRESS TX ##########################################################
/// Progress report transmitter.
#[derive(Clone)]
pub struct ProgressTx {
    publisher: Publisher<Progress>,
    cancel: Switch,
}

impl ProgressTx {
    /// Construct a new progress tramsmitter, with the given publisher and cancel switch.
    pub fn new(publisher: Publisher<Progress>, cancel_switch: Switch) -> Self {
        Self {
            publisher,
            cancel: cancel_switch,
        }
    }

    /// Create a dummy progress transmitter with no link to a broadcasting system.
    pub fn dummy() -> Self {
        Self::new(crossbeam_channel::unbounded().0, Switch::off())
    }

    /// Send a progress report.
    ///
    /// # Example
    /// ```rust
    /// # use divvy::*;
    /// let dummy = ProgressTx::dummy();
    /// dummy.send(None, "A message with no percent");
    /// dummy.send(50, "A message with 50%");
    /// ```
    pub fn send<P, M>(&self, pct: P, msg: M)
    where
        P: Into<Option<u8>>,
        M: Into<Cow<'static, str>>,
    {
        let pct = pct.into().unwrap_or(0);
        let msg = msg.into();
        self.send_report(Progress { msg, pct });
    }

    /// Send a progress report. It is recommended to use [`ProgressTx::send`] where possible.
    pub fn send_report(&self, progress: Progress) {
        self.publisher.send(progress).ok();
    }

    /// Flag has been set to cancel the current processing.
    pub fn cancelled(&self) -> bool {
        self.cancel.get()
    }
}

// ###### BROADCAST ############################################################
/// A broadcasting topic, which can be subscribed or published to.
#[derive(Default)]
pub struct Topic<T> {
    subscribers: Arc<Mutex<Vec<Sender<T>>>>,
}

/// A publisher, able to send to the topic to be broadcast.
pub type Publisher<T> = Sender<T>;
/// A subscriber, able to receive from the topic when a broadcast happens.
pub type Subscriber<T> = Receiver<T>;

impl<T> Topic<T> {
    /// Create a new topic, which can be subscribed and published to.
    pub fn new() -> Self {
        Topic {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add a subscriber to the topic.
    ///
    /// If the topic has been poisoned, an error will be returned.
    pub fn subscribe(&mut self) -> Subscriber<T> {
        let (tx, rx) = unbounded();
        let mut subs = self.subscribers.lock();
        subs.push(tx);
        rx
    }

    /// Add a publisher to the topic.
    pub fn add_publisher(&mut self) -> Publisher<T>
    where
        T: Clone + Send + 'static,
    {
        let (tx, rx) = unbounded();
        let subs = Arc::clone(&self.subscribers);
        std::thread::spawn(move || recv_publications(rx, &subs));
        tx
    }
}

fn recv_publications<T: Clone>(publisher: Receiver<T>, subs: &Mutex<Vec<Sender<T>>>) {
    // receives until channel becomes empty and disconnected
    for publication in publisher.iter() {
        send_or_remove(&mut subs.lock(), publication);
    }
}

fn send_or_remove<T: Clone>(subscribers: &mut Vec<Sender<T>>, item: T) {
    if subscribers.is_empty() {
        return;
    }

    let mut i = 0;
    while i < (subscribers.len() - 1) {
        match subscribers[i].send(item.clone()) {
            Ok(_) => i += 1,
            Err(_) => {
                subscribers.remove(i);
            }
        }
    }

    if !subscribers.is_empty() {
        debug_assert_eq!(subscribers.len() - 1, i);
        if subscribers[i].send(item).is_err() {
            subscribers.remove(i);
        }
    }
}
