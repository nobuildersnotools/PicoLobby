use futures::stream::Stream;
use net::raw_packet::RawPacket;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

type AsyncClosure<T> =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = T> + Send>> + Send + 'static>;

enum Producer<T> {
    Item(T),
    RawPacketCache(Arc<[RawPacket]>),
    SyncClosure(Box<dyn FnOnce() -> T + Send + 'static>),
    AsyncClosure(AsyncClosure<T>),
    Iterator(Box<dyn Iterator<Item = T> + Send + 'static>),
}

pub enum OutboundPacket<T> {
    Registry(T),
    Raw(RawPacket),
}

pub struct Batch<T> {
    producers: Vec<Producer<T>>,
}

impl<T: Send + 'static> Batch<T> {
    pub const fn new() -> Self {
        Self {
            producers: Vec::new(),
        }
    }

    #[cfg(feature = "bench_support")]
    #[allow(dead_code)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            producers: Vec::with_capacity(capacity),
        }
    }

    /// Queues an already-built item without allocating a closure.
    pub fn push_item(&mut self, item: T) {
        self.producers.push(Producer::Item(item));
    }

    /// Queues a synchronous function or closure.
    pub fn queue<F>(&mut self, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        self.producers.push(Producer::SyncClosure(Box::new(f)));
    }

    /// Queues an async closure that may or may not produce a value.
    pub fn queue_async<F, Fut>(&mut self, f: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let closure = move || -> Pin<Box<dyn Future<Output = T> + Send>> { Box::pin(f()) };
        self.producers
            .push(Producer::AsyncClosure(Box::new(closure)));
    }

    /// Chains a synchronous iterator.
    pub fn chain_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Send + 'static,
    {
        self.producers
            .push(Producer::Iterator(Box::new(iter.into_iter())));
    }

    /// Queues cached raw packets.  Each packet is cheap to clone because
    /// `RawPacket` stores its bytes behind an `Arc`.
    pub fn chain_raw_packet_cache(&mut self, packets: Arc<[RawPacket]>) {
        self.producers.push(Producer::RawPacketCache(packets));
    }

    /// Drains all synchronous producers into a `Vec`.  Panics if any producer is
    /// async or raw-only (futures/iterators that haven't been resolved yet).  Only used in tests.
    #[cfg(test)]
    pub fn into_vec(self) -> Vec<T> {
        let mut out = Vec::new();
        for producer in self.producers {
            match producer {
                Producer::Item(item) => out.push(item),
                Producer::SyncClosure(f) => out.push(f()),
                Producer::Iterator(iter) => out.extend(iter),
                Producer::RawPacketCache(_) => {
                    panic!("into_vec called on a Batch containing raw packets")
                }
                Producer::AsyncClosure(_) => {
                    panic!("into_vec called on a Batch containing async producers")
                }
            }
        }
        out
    }

    #[cfg(test)]
    pub fn into_stream(self) -> RegistryBatchStream<T> {
        RegistryBatchStream {
            inner: self.into_outbound_stream(),
        }
    }

    pub fn into_outbound_stream(self) -> BatchStream<T> {
        BatchStream {
            producers: self.producers.into_iter(),
            current: Current::Idle,
        }
    }
}

enum Current<T> {
    Idle,
    Future(Pin<Box<dyn Future<Output = T> + Send>>),
    Iterator(Box<dyn Iterator<Item = T> + Send>),
    RawPacketCache {
        packets: Arc<[RawPacket]>,
        index: usize,
    },
}

pub struct BatchStream<T> {
    producers: std::vec::IntoIter<Producer<T>>,
    current: Current<T>,
}

#[cfg(test)]
pub struct RegistryBatchStream<T> {
    inner: BatchStream<T>,
}

#[cfg(test)]
impl<T: Send + Unpin + 'static> Stream for RegistryBatchStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            let item = futures::ready!(Pin::new(&mut this.inner).poll_next(cx));
            match item {
                Some(OutboundPacket::Registry(packet)) => return Poll::Ready(Some(packet)),
                Some(OutboundPacket::Raw(_)) => {}
                None => return Poll::Ready(None),
            }
        }
    }
}

impl<T: Send + Unpin + 'static> Stream for BatchStream<T> {
    type Item = OutboundPacket<T>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            match &mut this.current {
                Current::Future(fut) => {
                    return match fut.as_mut().poll(cx) {
                        Poll::Ready(item) => {
                            this.current = Current::Idle;
                            Poll::Ready(Some(OutboundPacket::Registry(item)))
                        }
                        Poll::Pending => Poll::Pending,
                    };
                }
                Current::Iterator(iter) => {
                    if let Some(item) = iter.next() {
                        return Poll::Ready(Some(OutboundPacket::Registry(item)));
                    }
                    this.current = Current::Idle;
                }
                Current::RawPacketCache { packets, index } => {
                    if let Some(packet) = packets.get(*index) {
                        *index += 1;
                        return Poll::Ready(Some(OutboundPacket::Raw(packet.clone())));
                    }
                    this.current = Current::Idle;
                }
                Current::Idle => match this.producers.next() {
                    Some(Producer::Item(item)) => {
                        return Poll::Ready(Some(OutboundPacket::Registry(item)));
                    }
                    Some(Producer::RawPacketCache(packets)) => {
                        this.current = Current::RawPacketCache { packets, index: 0 };
                    }
                    Some(Producer::SyncClosure(f)) => {
                        return Poll::Ready(Some(OutboundPacket::Registry(f())));
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

        let mut stream = batch.into_outbound_stream();

        assert!(matches!(
            stream.next().await,
            Some(OutboundPacket::Registry(1))
        ));
        assert!(matches!(
            stream.next().await,
            Some(OutboundPacket::Registry(2))
        ));
        assert!(matches!(
            stream.next().await,
            Some(OutboundPacket::Registry(3))
        ));
        assert!(matches!(
            stream.next().await,
            Some(OutboundPacket::Registry(4))
        ));
        assert!(stream.next().await.is_none());
    }
}
