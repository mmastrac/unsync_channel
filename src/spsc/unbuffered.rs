use std::future::poll_fn;
use std::task::{Poll, Waker, Context};
use std::ptr::NonNull;

struct Shared<T> {
    refcount: u8,
    rx_waker: Option<Waker>,
    tx_waker: Option<Waker>,
    pending: Option<T>,
}

/// The sender side of an unbuffered SPSC [`channel`].
pub struct Sender<T> {
    shared: NonNull<Shared<T>>,
}

impl <T> Drop for Sender<T> {
    fn drop(&mut self) {
        // SAFETY: There's only ever one of these since we're !Send/!Sync
        let shared = unsafe { self.shared.as_mut() };
        if shared.refcount == 1 {
            // SAFETY: We're dropping here and we no it's no longer ref'd
            unsafe { Box::from_raw(shared) };
        } else {
            shared.refcount -= 1;
        }
    }
}

impl <T> Sender<T> {
    /// Asynchronously poll to place an item in the chnnale. If no root is available, registers the
    /// waker for when there is room.
    pub fn poll_send(&mut self, cx: &mut Context, t: &mut Option<T>) -> Poll<bool> {
        let shared = unsafe { self.shared.as_mut() };
        if shared.refcount == 1 {
            Poll::Ready(false)
        } else if shared.pending.is_some() {
            shared.tx_waker = Some(cx.waker().clone());
            Poll::Pending
        } else {
            shared.pending = std::mem::take(t);
            shared.tx_waker.take();
            if let Some(rx) = shared.rx_waker.take() {
                rx.wake();
            }
            Poll::Ready(true)
        }
    }

    /// Asynchronously send an item. Cancellation unsafe, as the item will be
    /// lost if the send could not complete.
    pub async fn send(&mut self, t: T) -> Result<(), T> {
        let mut t = Some(t);
        poll_fn(|cx| self.poll_send(cx, &mut t)).await;
        if let Some(t) = t {
            Err(t)
        } else {
            Ok(())
        }
    }

    /// Synchronously tries to send an item, if room is available. If no room is available,
    /// returns Err(T) with the item.
    pub fn try_send(&mut self, t: T) -> Result<(), T> {
        // SAFETY: There's only ever one of these since we're !Send/!Sync
        let shared = unsafe { self.shared.as_mut() };
        if let Some(t) = shared.pending.take() {
            Err(t)
        } else {
            shared.pending = Some(t);
            if let Some(rx) = shared.rx_waker.take() {
                rx.wake();
            }
            Ok(())
        }
    }
}

/// The receiver side of an unbuffered SPSC [`channel`].
pub struct Receiver<T> {
    shared: NonNull<Shared<T>>,
}

impl <T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // SAFETY: There's only ever one of these since we're !Send/!Sync
        let shared = unsafe { self.shared.as_mut() };
        if shared.refcount == 1 {
            // SAFETY: We're dropping here and we no it's no longer ref'd
            unsafe { Box::from_raw(shared) };
        } else {
            shared.refcount -= 1;
        }
    }
}   

impl <T> Receiver<T> {
    /// Asynchronously poll for an item in the channel. If no item is available, registers the
    /// waker for when there is an item.
    pub fn poll_recv(&mut self, cx: &mut Context) -> Poll<Option<T>> {
        // SAFETY: There's only ever one of these since we're !Send/!Sync
        let shared = unsafe { self.shared.as_mut() };
        if let Some(item) = shared.pending.take() {
            shared.rx_waker.take();
            if let Some(tx) = shared.tx_waker.take() {
                tx.wake();
            }
            Poll::Ready(Some(item))
        } else if shared.refcount == 1 {
            Poll::Ready(None)
        } else {
            shared.rx_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    /// Synchronously try to receive an item, if one is available.
    pub fn try_recv(&mut self) -> Option<T> {
        // SAFETY: There's only ever one of these since we're !Send/!Sync
        let shared = unsafe { self.shared.as_mut() };
        let res = shared.pending.take();
        if let Some(tx) = shared.tx_waker.take() {
            tx.wake();
        }
        res
    }
    
    /// Asynchronously receive an item. Cancellation-safe.
    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| self.poll_recv(cx)).await
    }
}

/// A single-producer, single-consumer, single-item channel. Benchmarks suggest that this has much lower overhead than
/// `tokio::sync::mpsc::channel`: in the case of a `LocalSet` sending and receiving between two tasks, it
/// takes about 75ns to send 10 items. For the equivalent tokio channel, it takes approximately 412ns.
/// 
/// Using the try_send and try_recv APIs, this queue takes 13ns to send and receive an item, versus 399ns for
/// the equivalent tokio channel.
/// 
/// Note that this channel has room for one and only one item. Should the item in the channel be unreceived, send attempts
/// will block. As this is a SPSC-style channel, however, fairness is not a concern.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Box::new(Shared {
        refcount: 2,
        rx_waker: None,
        tx_waker: None,
        pending: None
    });
    // SAFETY: We know this is non-null since we just allocated it
    let shared = unsafe { NonNull::new_unchecked(Box::into_raw(shared)) };
    let send = Sender {
        shared,
    };
    let recv = Receiver {
        shared
    };

    (send, recv)
}

#[cfg(test)]
mod tests {
    use futures::task::noop_waker_ref;
    use tokio::task::LocalSet;
    use super::*;

    macro_rules! ctx {
        () => {
            &mut Context::from_waker(noop_waker_ref())
        };
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_send() {
        let (mut tx, _rx) = channel::<u32>();
        let mut to_send = Some(1);
        // We can send one item buffered
        assert_eq!(tx.poll_send(ctx!(), &mut to_send), Poll::Ready(true));
        let mut to_send = Some(1);
        assert!(tx.poll_send(ctx!(), &mut to_send).is_pending());
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_send_recv() {
        let (mut tx, mut rx) = channel::<u32>();
        let mut to_send = Some(1);
        // We can send one item buffered
        assert_eq!(tx.poll_send(ctx!(), &mut to_send), Poll::Ready(true));
        let mut to_send = Some(1);
        assert!(tx.poll_send(ctx!(), &mut to_send).is_pending());
        assert_eq!(rx.poll_recv(ctx!()), Poll::Ready(Some(1)));
        assert!(rx.poll_recv(ctx!()).is_pending());
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_send_drop_still_recv() {
        let (mut tx, mut rx) = channel::<u32>();
        let mut to_send = Some(1);
        // We can send one item buffered
        assert_eq!(tx.poll_send(ctx!(), &mut to_send), Poll::Ready(true));
        drop(tx);
        assert_eq!(rx.poll_recv(ctx!()), Poll::Ready(Some(1)));
        assert_eq!(rx.poll_recv(ctx!()), Poll::Ready(None));
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_send_no_rx() {
        let (mut tx, rx) = channel::<u32>();
        drop(rx);
        let mut to_send = Some(1);
        assert_eq!(tx.poll_send(ctx!(), &mut to_send), Poll::Ready(false));
        let mut to_send = Some(1);
        assert_eq!(tx.poll_send(ctx!(), &mut to_send), Poll::Ready(false));
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_send_lots() {
        const COUNT: usize = 100;
        let (mut tx, mut rx) = channel();
        let local = LocalSet::new();
        local.spawn_local(async move {
            for i in 0..COUNT {
                if i % 5 == 0 {
                    tokio::task::yield_now().await;
                }
                tx.send(i).await.unwrap();
            }
        });
        local.spawn_local(async move {
            for i in 0..COUNT {
                if i % 3 == 0 {
                    tokio::task::yield_now().await;
                }
                assert_eq!(rx.recv().await, Some(i));
            }
            assert!(rx.recv().await.is_none());
        });
    }
}
