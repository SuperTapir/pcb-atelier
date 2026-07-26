use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    guard_id: Uuid,
    counter: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct RevisionGuard {
    guard_id: Uuid,
    counter: u64,
}

impl Default for RevisionGuard {
    fn default() -> Self {
        Self {
            guard_id: Uuid::new_v4(),
            counter: 0,
        }
    }
}

impl RevisionGuard {
    pub fn issue(&mut self) -> Result<Revision, RevisionOverflow> {
        self.counter = self.counter.checked_add(1).ok_or(RevisionOverflow)?;
        Ok(Revision {
            guard_id: self.guard_id,
            counter: self.counter,
        })
    }

    pub fn is_current(&self, revision: Revision) -> bool {
        revision.guard_id == self.guard_id && revision.counter == self.counter
    }

    pub fn accept<Value>(&self, revision: Revision, value: Value) -> Option<Value> {
        self.is_current(revision).then_some(value)
    }

    #[doc(hidden)]
    pub fn from_counter_for_test(counter: u64) -> Self {
        Self {
            guard_id: Uuid::new_v4(),
            counter,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("revision counter exhausted")]
pub struct RevisionOverflow;
