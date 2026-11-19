// One session, one place: the root process each session was first seen at, kept in a file
// beside the database (its name: common/home.rs), so that a restart does not forget.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::common::home::PINS;

/// The root a session was first seen at.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pin {
    /// The machine the root is on; none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub place: String,
    pub pid: u32,
    pub start: u64,
}

/// One session, one place: each session with the root it was first seen at, kept in a file
/// next to the database so that a restart does not forget.
pub struct Pins {
    path: PathBuf,
    map: Mutex<BTreeMap<String, Pin>>,
}

impl Pins {
    pub fn open(home: &Path) -> Pins {
        let path = home.join(PINS);
        let map = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Pins { path, map: Mutex::new(map) }
    }

    pub fn get(&self, session: &str) -> Option<Pin> {
        self.map.lock().unwrap().get(session).cloned()
    }

    pub fn all(&self) -> Vec<(String, Pin)> {
        self.map.lock().unwrap().iter().map(|(s, p)| (s.clone(), p.clone())).collect()
    }

    /// Sets (`Some`) or releases (`None`) pins, and writes the file when anything changed.
    pub fn change(&self, changes: Vec<(String, Option<Pin>)>) {
        let mut map = self.map.lock().unwrap();
        let before = map.clone();
        for (session, pin) in changes {
            match pin {
                Some(p) => map.insert(session, p),
                None => map.remove(&session),
            };
        }
        if *map != before
            && let Err(e) = serde_json::to_vec_pretty(&*map).map_err(std::io::Error::other).and_then(|b| std::fs::write(&self.path, b))
        {
            tracing::warn!("cannot write {}: {e}", self.path.display());
        }
    }
}
