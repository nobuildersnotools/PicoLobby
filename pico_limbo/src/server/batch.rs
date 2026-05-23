use futures::stream::Stream;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

type AsyncClosure<T> =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = T> + Send>> + Send + 'static>;

enum Producer<T> {
    Item(T),
    SyncClosure(Box<dyn FnOnce() -> T + Send + 'static>),
    AsyncClosure(AsyncClosure<T>),
    Iterator(Box<dyn Iterator<Item = T> + Send + 'static>),
}

pub struct Batch<T> {
    producers: VecDeque<Producer<T>>,
}

impl<T: Send + 'static> Batch<T> {
    pub const fn new() -> Self {
        Self {
            producers: VecDeque::new(),
        }
    }

    /// Queues an already-built item without allocating a closure.
    pub fn push_item(&mut self, item: T) {
        self.producers.push_back(Producer::Item(item));
    }

    /// Queues a synchronous function or closure.
    pub fn queue<F>(&mut self, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        self.producers.push_back(Producer::SyncClosure(Box::new(f)));
    }

    /// Queues an async closure that may or may not produce a value.
    pub fn queue_async<F, Fut>(&mut self, f: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let closure = move || -> Pin<Box<dyn Future<Output = T> + Send>> { Box::pin(f()) };
        self.producers
            .push_back(Producer::AsyncClosure(Box::new(closure)));
    }

    /// Chains a synchronous iterator.
    pub fn chain_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Send + 'static,
    {
        self.producers
            .push_back(Producer::Iterator(Box::new(iter.into_iter())));
    }

    /// Drains all synchronous producers into a `Vec`.  Panics if any producer is
    /// async (futures/iterators that haven't been resolved yet).  Only used in tests.
    #[cfg(test)]
    pub fn into_vec(self) -> Vec<T> {
        let mut out = Vec::new();
        for producer in self.producers {
            match producer {
                Producer::Item(item) => out.push(item),
                Producer::SyncClosure(f) => out.push(f()),
                Producer::Iterator(iter) => out.extend(iter),
                Producer::AsyncClosure(_) => {
                    panic!("into_vec called on a Batch containing async producers")
                }
            }
        }
        out
    }

    pub fn into_stream(self) -> BatchStream<T> {
        BatchStream {
            producers: self.producers,
            current: Current::Idle,
        }
    }
}

enum Current<T> {
    Idle,
    Future(Pin<Box<dyn Future<Output = T> + Send>>),
    Iterator(Box<dyn Iterator<Item = T> + Send>),
}

pub struct BatchStream<T> {
    producers: VecDeque<Producer<T>>,
    current: Current<T>,
}

impl<T: Send + Unpin + 'static> Stream for BatchStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            match &mut this.current {
                Current::Future(fut) => {
                    return match fut.as_mut().poll(cx) {
                        Poll::Ready(item) => {
                            this.current = Current::Idle;
                            Poll::Ready(Some(item))
                        }
                        Poll::Pending => Poll::Pending,
                    };
                }
                Current::Iterator(iter) => {
                    if let Some(item) = iter.next() {
                        return Poll::Ready(Some(item));
                    }
                    this.current = Current::Idle;
                }
                Current::Idle => match this.producers.pop_front() {
                    Some(Producer::Item(item)) => {
                        return Poll::Ready(Some(item));
                    }
                    Some(Producer::SyncClosure(f)) => {
                        return Poll::Ready(Some(f()));
                    }
                    Some(Producer::AsyncClosure(f)) => {
                        this.current = Current::Future(f());
                    }
                    Some(Producer::Iterator(iter)) => {
                        this.current = Current::Iterator(iter);
                    }
                    None => {
                        return Poll::Ready(None);
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[tokio::test]
    async fn test_batch_stream() {
        let mut batch = Batch::new();

        batch.queue(|| 1);
        batch.queue_async(|| async { 2 });
        batch.chain_iter(3..5);

        let mut stream = batch.into_stream();

        assert_eq!(stream.next().await, Some(1));
        assert_eq!(stream.next().await, Some(2));
        assert_eq!(stream.next().await, Some(3));
        assert_eq!(stream.next().await, Some(4));
        assert_eq!(stream.next().await, None);
    }
}
