use crate::catalogue;
use crate::core::Core;
use crate::error::Result;
use crate::payoff;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{Notify, oneshot};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Priority {
    Waiting,
    FollowUp,
    Refill,
    TagDetails,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Job {
    Page {
        query: String,
        page: i64,
        max_age: i64,
        lead: Option<String>,
    },
    TagDetails,
}

struct Queued {
    job: Job,
    priority: Priority,
    waiters: Vec<oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct FetchQueue {
    queued: Mutex<Vec<Queued>>,
    ready: Notify,
}

impl FetchQueue {
    pub fn push(&self, job: Job, priority: Priority) {
        self.enqueue(job, priority, None);
    }

    pub async fn run_now(&self, jobs: Vec<Job>) {
        let receivers: Vec<oneshot::Receiver<()>> = jobs
            .into_iter()
            .map(|job| {
                let (sender, receiver) = oneshot::channel();
                self.enqueue(job, Priority::Waiting, Some(sender));
                receiver
            })
            .collect();
        for receiver in receivers {
            let _ = receiver.await;
        }
    }

    pub fn has(&self, priority: Priority) -> bool {
        self.lock().iter().any(|queued| queued.priority == priority)
    }

    pub fn wake(&self) {
        self.ready.notify_one();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Queued>> {
        self.queued
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn enqueue(&self, job: Job, priority: Priority, waiter: Option<oneshot::Sender<()>>) {
        {
            let mut queued = self.lock();
            match queued.iter_mut().find(|queued| queued.job == job) {
                Some(existing) => {
                    existing.priority = existing.priority.min(priority);
                    existing.waiters.extend(waiter);
                }
                None => queued.push(Queued {
                    job,
                    priority,
                    waiters: waiter.into_iter().collect(),
                }),
            }
        }
        self.wake();
    }

    fn next(&self) -> Option<Queued> {
        let mut queued = self.lock();
        let index = queued
            .iter()
            .enumerate()
            .min_by_key(|(index, queued)| (queued.priority, *index))
            .map(|(index, _)| index)?;
        Some(queued.remove(index))
    }
}

pub fn start(core: &Arc<Core>) {
    let queue = core.queue.clone();
    let core = Arc::downgrade(core);
    crate::platform::spawn(async move {
        while let Some(queued) = next_job(&queue, &core).await {
            if let Some(core) = core.upgrade()
                && let Err(error) = run(&core, &queued.job).await
            {
                crate::platform::log(&format!("Background fetch {:?} failed: {error}", queued.job));
            }
            for waiter in queued.waiters {
                let _ = waiter.send(());
            }
        }
    });
}

async fn next_job(queue: &FetchQueue, core: &Weak<Core>) -> Option<Queued> {
    loop {
        if core.strong_count() == 0 {
            return None;
        }
        if let Some(queued) = queue.next() {
            return Some(queued);
        }
        queue.ready.notified().await;
    }
}

async fn run(core: &Arc<Core>, job: &Job) -> Result<()> {
    match job {
        Job::Page {
            query,
            page,
            max_age,
            lead,
        } => {
            let stored = catalogue::fetch_page(core, query, *page, *max_age).await?;
            if let (Some(tag), Some(stored)) = (lead, stored) {
                payoff::credit(&core.db.conn(), tag, &stored.ids)?;
            }
            Ok(())
        }
        Job::TagDetails => catalogue::fill_tag_details_batch(core).await,
    }
}
