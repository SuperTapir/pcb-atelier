use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
};

use atelier_core::{CompiledImageTreatment, TreatmentId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TreatmentJobKey {
    pub treatment_id: TreatmentId,
    pub revision: u64,
    pub recipe_fingerprint: String,
    pub cache_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LatestRequest {
    revision: u64,
    recipe_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreatmentJobError {
    Cancelled,
    Stale,
    Failed(String),
}

type JobResult = Result<Arc<CompiledImageTreatment>, TreatmentJobError>;

#[derive(Default)]
struct JobSlot {
    result: Mutex<Option<JobResult>>,
    ready: Condvar,
}

struct SchedulerState {
    active: usize,
    minimum_revision: u64,
    latest: HashMap<TreatmentId, LatestRequest>,
    in_flight: HashMap<TreatmentJobKey, Arc<JobSlot>>,
    cache: HashMap<TreatmentJobKey, Arc<CompiledImageTreatment>>,
}

pub struct TreatmentProcessingScheduler {
    concurrency_limit: usize,
    state: Mutex<SchedulerState>,
    capacity: Condvar,
}

impl TreatmentProcessingScheduler {
    pub fn new(concurrency_limit: usize) -> Self {
        assert!(concurrency_limit > 0, "concurrency limit must be positive");
        Self {
            concurrency_limit,
            state: Mutex::new(SchedulerState {
                active: 0,
                minimum_revision: 0,
                latest: HashMap::new(),
                in_flight: HashMap::new(),
                cache: HashMap::new(),
            }),
            capacity: Condvar::new(),
        }
    }

    pub fn advance_revision(&self, revision: u64) {
        let mut state = self.state.lock().expect("treatment scheduler lock");
        state.minimum_revision = state.minimum_revision.max(revision);
        let minimum_revision = state.minimum_revision;
        state
            .cache
            .retain(|key, _| key.revision >= minimum_revision);
        self.capacity.notify_all();
    }

    pub fn compile(
        &self,
        key: TreatmentJobKey,
        work: impl FnOnce() -> Result<CompiledImageTreatment, String>,
    ) -> JobResult {
        let (slot, owner) = {
            let mut state = self.state.lock().expect("treatment scheduler lock");
            if key.revision < state.minimum_revision {
                return Err(TreatmentJobError::Cancelled);
            }
            match state.latest.get(&key.treatment_id) {
                Some(latest) if latest.revision > key.revision => {
                    return Err(TreatmentJobError::Cancelled);
                }
                Some(latest)
                    if latest.revision == key.revision
                        && latest.recipe_fingerprint != key.recipe_fingerprint =>
                {
                    return Err(TreatmentJobError::Cancelled);
                }
                _ => {
                    state.latest.insert(
                        key.treatment_id,
                        LatestRequest {
                            revision: key.revision,
                            recipe_fingerprint: key.recipe_fingerprint.clone(),
                        },
                    );
                }
            }
            if let Some(cached) = state.cache.get(&key) {
                return Ok(Arc::clone(cached));
            }
            if let Some(slot) = state.in_flight.get(&key) {
                (Arc::clone(slot), false)
            } else {
                let slot = Arc::new(JobSlot::default());
                state.in_flight.insert(key.clone(), Arc::clone(&slot));
                (slot, true)
            }
        };

        if !owner {
            let mut result = slot.result.lock().expect("treatment job result lock");
            while result.is_none() {
                result = slot.ready.wait(result).expect("treatment job result wait");
            }
            return result.clone().expect("ready treatment job result");
        }

        {
            let mut state = self.state.lock().expect("treatment scheduler lock");
            while state.active >= self.concurrency_limit && is_current(&state, &key) {
                state = self
                    .capacity
                    .wait(state)
                    .expect("treatment scheduler capacity wait");
            }
            if !is_current(&state, &key) {
                state.in_flight.remove(&key);
                drop(state);
                complete_slot(&slot, Err(TreatmentJobError::Cancelled));
                return Err(TreatmentJobError::Cancelled);
            }
            state.active += 1;
        }

        let compiled = work().map(Arc::new).map_err(TreatmentJobError::Failed);

        let accepted = {
            let mut state = self.state.lock().expect("treatment scheduler lock");
            state.active -= 1;
            let accepted = if is_current(&state, &key) {
                if let Ok(compiled) = &compiled {
                    state.cache.insert(key.clone(), Arc::clone(compiled));
                }
                compiled
            } else {
                Err(TreatmentJobError::Stale)
            };
            state.in_flight.remove(&key);
            self.capacity.notify_all();
            accepted
        };
        complete_slot(&slot, accepted.clone());
        accepted
    }

    #[cfg(test)]
    fn cached_job_count(&self) -> usize {
        self.state
            .lock()
            .expect("treatment scheduler lock")
            .cache
            .len()
    }

    #[cfg(test)]
    fn latest_revision(&self, treatment_id: TreatmentId) -> Option<u64> {
        self.state
            .lock()
            .expect("treatment scheduler lock")
            .latest
            .get(&treatment_id)
            .map(|request| request.revision)
    }
}

fn is_current(state: &SchedulerState, key: &TreatmentJobKey) -> bool {
    key.revision >= state.minimum_revision
        && state.latest.get(&key.treatment_id).is_some_and(|latest| {
            latest.revision == key.revision && latest.recipe_fingerprint == key.recipe_fingerprint
        })
}

fn complete_slot(slot: &JobSlot, result: JobResult) {
    *slot.result.lock().expect("treatment job result lock") = Some(result);
    slot.ready.notify_all();
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Barrier, Condvar, Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    use atelier_core::{
        BitMask, CompiledImageTreatment, MaskTopology, PhysicalBoundsUm, SamplingPurpose,
        TreatmentId,
    };

    use super::{TreatmentJobError, TreatmentJobKey, TreatmentProcessingScheduler};

    fn key(treatment_id: TreatmentId, revision: u64) -> TreatmentJobKey {
        TreatmentJobKey {
            treatment_id,
            revision,
            recipe_fingerprint: format!("recipe-{revision}"),
            cache_key: format!("cache-{revision}"),
        }
    }

    fn compiled(revision: u64) -> CompiledImageTreatment {
        CompiledImageTreatment {
            mask: BitMask::new(1, 1).expect("mask"),
            applied_threshold: 128,
            pixel_pitch_um: 25,
            bounds_um: PhysicalBoundsUm {
                min_x_um: 0,
                min_y_um: 0,
                max_x_um: 25,
                max_y_um: 25,
            },
            recipe_fingerprint: format!("recipe-{revision}"),
            revision,
            purpose: SamplingPurpose::InteractiveProxy,
            topology: MaskTopology {
                island_count: 0,
                hole_count: 0,
            },
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn identical_object_jobs_are_coalesced() {
        let scheduler = Arc::new(TreatmentProcessingScheduler::new(2));
        let treatment_id = TreatmentId::new();
        let job_key = key(treatment_id, 1);
        let executions = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Barrier::new(2));
        let (release_tx, release_rx) = mpsc::channel();

        let first = {
            let scheduler = Arc::clone(&scheduler);
            let executions = Arc::clone(&executions);
            let started = Arc::clone(&started);
            let job_key = job_key.clone();
            thread::spawn(move || {
                scheduler.compile(job_key, || {
                    executions.fetch_add(1, Ordering::SeqCst);
                    started.wait();
                    release_rx.recv().expect("release first job");
                    Ok(compiled(1))
                })
            })
        };
        started.wait();
        let second = {
            let scheduler = Arc::clone(&scheduler);
            let executions = Arc::clone(&executions);
            thread::spawn(move || {
                scheduler.compile(job_key, || {
                    executions.fetch_add(1, Ordering::SeqCst);
                    Ok(compiled(1))
                })
            })
        };
        release_tx.send(()).expect("release job");

        assert!(first.join().expect("first thread").is_ok());
        assert!(second.join().expect("second thread").is_ok());
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(scheduler.cached_job_count(), 1);
    }

    #[test]
    fn queued_obsolete_job_is_cancelled_before_work_starts() {
        let scheduler = Arc::new(TreatmentProcessingScheduler::new(1));
        let blocker_id = TreatmentId::new();
        let target_id = TreatmentId::new();
        let blocker_started = Arc::new(Barrier::new(2));
        let (release_tx, release_rx) = mpsc::channel();
        let blocker = {
            let scheduler = Arc::clone(&scheduler);
            let blocker_started = Arc::clone(&blocker_started);
            thread::spawn(move || {
                scheduler.compile(key(blocker_id, 1), || {
                    blocker_started.wait();
                    release_rx.recv().expect("release blocker");
                    Ok(compiled(1))
                })
            })
        };
        blocker_started.wait();

        let obsolete_executions = Arc::new(AtomicUsize::new(0));
        let obsolete = {
            let scheduler = Arc::clone(&scheduler);
            let executions = Arc::clone(&obsolete_executions);
            thread::spawn(move || {
                scheduler.compile(key(target_id, 1), || {
                    executions.fetch_add(1, Ordering::SeqCst);
                    Ok(compiled(1))
                })
            })
        };
        let current = {
            let scheduler = Arc::clone(&scheduler);
            thread::spawn(move || scheduler.compile(key(target_id, 2), || Ok(compiled(2))))
        };
        while scheduler.latest_revision(target_id) != Some(2) {
            thread::yield_now();
        }
        release_tx.send(()).expect("release blocker");

        assert!(blocker.join().expect("blocker thread").is_ok());
        assert_eq!(
            obsolete.join().expect("obsolete thread"),
            Err(TreatmentJobError::Cancelled)
        );
        assert!(current.join().expect("current thread").is_ok());
        assert_eq!(obsolete_executions.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn late_result_is_rejected_and_never_cached() {
        let scheduler = Arc::new(TreatmentProcessingScheduler::new(2));
        let treatment_id = TreatmentId::new();
        let old_started = Arc::new(Barrier::new(2));
        let (release_tx, release_rx) = mpsc::channel();
        let old = {
            let scheduler = Arc::clone(&scheduler);
            let old_started = Arc::clone(&old_started);
            thread::spawn(move || {
                scheduler.compile(key(treatment_id, 1), || {
                    old_started.wait();
                    release_rx.recv().expect("release old");
                    Ok(compiled(1))
                })
            })
        };
        old_started.wait();
        let current = scheduler
            .compile(key(treatment_id, 2), || Ok(compiled(2)))
            .expect("current job");
        release_tx.send(()).expect("release old");

        assert_eq!(current.revision, 2);
        assert_eq!(
            old.join().expect("old thread"),
            Err(TreatmentJobError::Stale)
        );
        assert_eq!(scheduler.cached_job_count(), 1);
    }

    #[test]
    fn concurrency_limit_bounds_simultaneous_work() {
        let scheduler = Arc::new(TreatmentProcessingScheduler::new(2));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let (started_tx, started_rx) = mpsc::channel();
        let mut jobs = Vec::new();
        for index in 0..3 {
            let scheduler = Arc::clone(&scheduler);
            let gate = Arc::clone(&gate);
            let started_tx = started_tx.clone();
            jobs.push(thread::spawn(move || {
                scheduler.compile(key(TreatmentId::new(), 1), || {
                    started_tx.send(index).expect("report started job");
                    let (lock, ready) = &*gate;
                    let mut released = lock.lock().expect("gate lock");
                    while !*released {
                        released = ready.wait(released).expect("gate wait");
                    }
                    Ok(compiled(1))
                })
            }));
        }
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first job starts");
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("second job starts");
        assert!(
            started_rx.recv_timeout(Duration::from_millis(25)).is_err(),
            "third job must wait for scheduler capacity"
        );

        let (lock, ready) = &*gate;
        *lock.lock().expect("gate lock") = true;
        ready.notify_all();
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("third job starts after capacity is released");
        for job in jobs {
            assert!(job.join().expect("worker thread").is_ok());
        }
    }

    #[test]
    fn workspace_revision_advance_invalidates_running_results() {
        let scheduler = Arc::new(TreatmentProcessingScheduler::new(1));
        let treatment_id = TreatmentId::new();
        let started = Arc::new(Barrier::new(2));
        let (release_tx, release_rx) = mpsc::channel();
        let job = {
            let scheduler = Arc::clone(&scheduler);
            let started = Arc::clone(&started);
            thread::spawn(move || {
                scheduler.compile(key(treatment_id, 1), || {
                    started.wait();
                    release_rx.recv().expect("release job");
                    Ok(compiled(1))
                })
            })
        };
        started.wait();
        scheduler.advance_revision(2);
        release_tx.send(()).expect("release job");

        assert_eq!(
            job.join().expect("job thread"),
            Err(TreatmentJobError::Stale)
        );
        assert_eq!(scheduler.cached_job_count(), 0);
    }
}
