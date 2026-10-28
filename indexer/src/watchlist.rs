use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct WatchList {
    pub pubkeys: Arc<Mutex<HashSet<String>>>,
}

impl WatchList {
    pub fn new() -> Self {
        Self {
            pubkeys: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn add(&self, pubkey: String) {
        if let Ok(mut set) = self.pubkeys.lock() {
            set.insert(pubkey);
        }
    }

    pub fn get_all(&self) -> Vec<String> {
        if let Ok(set) = self.pubkeys.lock() {
            set.iter().cloned().collect()
        } else {
            vec![]
        }
    }
}
