use futures::future::{BoxFuture, FutureExt};
use futures::task::{waker_ref, ArcWake};
use shrev::{EventChannel, ReaderId};
use specs::{System, Write};
use std::future::Future;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

type AsyncFuture<A> = Arc<Mutex<Option<BoxFuture<'static, A>>>>;

struct Task<A> {
    future: AsyncFuture<A>,
    task_sender: SyncSender<Arc<Task<A>>>,
}

pub struct Executor<A>
where
    A: 'static,
{
    ready_queue: Receiver<Arc<Task<A>>>,
    task_sender: SyncSender<Arc<Task<A>>>,
    new_jobs_reader: Option<ReaderId<AsyncFuture<A>>>,
}

impl<A> ArcWake for Task<A> {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self
            .task_sender
            .send(arc_self.clone())
            .expect("Too many tasks queued");
    }
}

impl<A> Executor<A>
where
    A: Send + Sync,
{
    pub fn new() -> Self {
        const MAX_QUEUED_TASKS: usize = 10_000;
        let (task_sender, ready_queue) = sync_channel(MAX_QUEUED_TASKS);
        Self {
            task_sender,
            ready_queue,
            new_jobs_reader: None,
        }
    }
}

impl<'s, A> System<'s> for Executor<A>
where
    A: Send + Sync,
{
    type SystemData = (
        Write<'s, EventChannel<AsyncFuture<A>>>,
        Write<'s, EventChannel<A>>,
    );

    fn run(&mut self, (mut new_jobs, mut job_results): Self::SystemData) {
        let mut new_jobs_reader = self
            .new_jobs_reader
            .get_or_insert_with(|| new_jobs.register_reader());

        for new_job in new_jobs.read(&mut new_jobs_reader) {
            println!("New async job queued");
            self.task_sender
                .send(Arc::new(Task {
                    future: new_job.clone(),
                    task_sender: self.task_sender.clone(),
                }))
                .expect("Too many tasks queued");
        }

        for task in self.ready_queue.try_iter() {
            let mut future_slot = task.future.lock().unwrap();
            if let Some(mut future) = future_slot.take() {
                println!("Polling future");
                let waker = waker_ref(&task);
                let context = &mut Context::from_waker(&*waker);
                if let Poll::Ready(value) = future.as_mut().poll(context) {
                    println!("Result completed");
                    job_results.single_write(value);
                } else {
                    println!("Future is not yet ready");
                    *future_slot = Some(future);
                }
            }
        }
    }
}

pub fn run_async<A, F>(mut event_channel: Write<EventChannel<AsyncFuture<A>>>, future: F)
where
    F: Future<Output = A> + Send + 'static,
    A: Send + Sync + 'static,
{
    println!("Queueing job");
    event_channel.single_write(Arc::new(Mutex::new(Some(future.boxed()))));
}
