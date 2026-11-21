use crate::booru::{Order, Site, SiteQuery};
use crate::catalogue;
use crate::core::Core;
use crate::error::Result;
use crate::library::{self, TagList};
use crate::payoff;
use crate::search::Ratings;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{Notify, oneshot};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Priority {
    Waiting,
    FollowUp,
    Refill,
    TagDetails,
}

// a search in the app's terms. it's written in a site's terms only when a worker takes it, so whichever site is free
// runs it
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PageSearch {
    pub tags: Vec<String>,
    pub ratings: Ratings,
    pub order: Option<Order>,
    // the only site that can run it as asked, like Gelbooru for more tags than Danbooru allows
    pub only: Option<Site>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Job {
    Page {
        search: PageSearch,
        page: i64,
        max_age: i64,
        lead: Option<String>,
    },
    TagDetails,
}

impl Job {
    // tag details ask Danbooru first, so they stay with its worker
    fn runs_on(&self, site: Site, gelbooru_searches: bool) -> bool {
        match (self, site) {
            (Job::TagDetails, site) => site == Site::Danbooru,
            (Job::Page { search, .. }, Site::Gelbooru) => gelbooru_searches && search.only != Some(Site::Danbooru),
            (Job::Page { search, .. }, Site::Danbooru) => search.only != Some(Site::Gelbooru) || !gelbooru_searches,
        }
    }
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
        self.ready.notify_waiters();
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

    // the most urgent job this site's worker can run, oldest first
    fn next(&self, site: Site, gelbooru_searches: bool) -> Option<Queued> {
        let mut queued = self.lock();
        let index = queued
            .iter()
            .enumerate()
            .filter(|(_, queued)| queued.job.runs_on(site, gelbooru_searches))
            .min_by_key(|(index, queued)| (queued.priority, *index))
            .map(|(index, _)| index)?;
        Some(queued.remove(index))
    }
}

// one worker per site, so the two sites' requests run side by side
pub fn start(core: &Arc<Core>) {
    for site in Site::ALL {
        let queue = core.queue.clone();
        let core = Arc::downgrade(core);
        crate::platform::spawn(async move {
            while let Some(queued) = next_job(&queue, &core, site).await {
                if let Some(core) = core.upgrade()
                    && let Err(error) = run(&core, site, &queued.job).await
                {
                    crate::platform::log(&format!("Background fetch {:?} on {} failed: {error}", queued.job, site.name()));
                }
                for waiter in queued.waiters {
                    let _ = waiter.send(());
                }
            }
        });
    }
}

async fn next_job(queue: &FetchQueue, core: &Weak<Core>, site: Site) -> Option<Queued> {
    loop {
        // listening starts before looking, so a job queued in between still wakes this worker
        let ready = queue.ready.notified();
        tokio::pin!(ready);
        ready.as_mut().enable();
        let gelbooru_searches = core.upgrade()?.can_search(Site::Gelbooru);
        if let Some(queued) = queue.next(site, gelbooru_searches) {
            return Some(queued);
        }
        ready.await;
    }
}

async fn run(core: &Arc<Core>, site: Site, job: &Job) -> Result<()> {
    let note = match job {
        Job::Page { lead, .. } => lead.clone().unwrap_or_else(|| "page".to_string()),
        Job::TagDetails => "tag details".to_string(),
    };
    let _measured = crate::timing::measure("job", note);
    match job {
        Job::Page {
            search,
            page,
            max_age,
            lead,
        } => {
            let blacklist = library::tag_names(&core.db.conn(), TagList::Blacklist)?;
            let query = core
                .booru(site)
                .render(&SiteQuery {
                    tags: search.tags.clone(),
                    ratings: search.ratings,
                    blacklist: &blacklist,
                    order: search.order,
                })
                .await?;
            let stored = catalogue::fetch_page(core, site, &query, *page, *max_age).await?;
            if let (Some(tag), Some(stored)) = (lead, stored) {
                payoff::credit(&core.db.conn(), tag, &stored.ids)?;
            }
            Ok(())
        }
        Job::TagDetails => catalogue::fill_tag_details_batch(core).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(tag: &str, only: Option<Site>) -> Job {
        Job::Page {
            search: PageSearch {
                tags: vec![tag.to_owned()],
                ratings: Ratings::ALL,
                order: None,
                only,
            },
            page: 0,
            max_age: 0,
            lead: None,
        }
    }

    fn taken(queue: &FetchQueue, site: Site, gelbooru_searches: bool) -> Option<Job> {
        queue.next(site, gelbooru_searches).map(|queued| queued.job)
    }

    #[test]
    fn each_site_takes_the_next_job_it_can_run() {
        let queue = FetchQueue::default();
        queue.push(Job::TagDetails, Priority::TagDetails);
        queue.push(page("cat", None), Priority::Refill);
        queue.push(page("dog", None), Priority::Refill);
        queue.push(page("many_tags", Some(Site::Gelbooru)), Priority::FollowUp);
        assert_eq!(taken(&queue, Site::Danbooru, true), Some(page("cat", None)));
        assert_eq!(taken(&queue, Site::Gelbooru, true), Some(page("many_tags", Some(Site::Gelbooru))));
        assert_eq!(taken(&queue, Site::Gelbooru, true), Some(page("dog", None)));
        // tag details ask Danbooru first, so Gelbooru's worker leaves them
        assert_eq!(taken(&queue, Site::Gelbooru, true), None);
        assert_eq!(taken(&queue, Site::Danbooru, true), Some(Job::TagDetails));
    }

    #[test]
    fn without_a_gelbooru_key_danbooru_takes_everything() {
        let queue = FetchQueue::default();
        queue.push(page("many_tags", Some(Site::Gelbooru)), Priority::FollowUp);
        assert_eq!(taken(&queue, Site::Gelbooru, false), None);
        assert_eq!(taken(&queue, Site::Danbooru, false), Some(page("many_tags", Some(Site::Gelbooru))));
    }
}
