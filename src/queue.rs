//! Transmit any number of values of the same type, then (after transmitting all) proceed according to
//! a continuation session. The two sides, receiving and sending, are [`Dequeue`] and [`Enqueue`], respectively.
//!
//! The order of values popped out of [`Dequeue`] is the same as their push order into [`Enqueue`] -- first-in,
//! first-out.
//!
//! Pushing values into [`Enqueue`] is always non-blocking -- any number of values may be pushed without
//! waiting for [`Dequeue`] to receive them.
//!
//! These queue sessions are just a standardization of a common recursive pattern. They are fully equivalent to:
//!
//! ```
//! enum Queue<T, S: Session> {
//!     Item(T, Dequeue<T, S>),
//!     Closed(S),
//! }
//!
//! // naked definitions of the two sides of the queue
//! type Dequeue<T, S> = Recv<Queue<T, S>>;
//! type Enqueue<T, S> = Send<Queue<T, Dual<S>>>;
//! ```

use super::{
    exchange::{Recv, Send},
    Session,
};
use futures::Future;
use std::marker;

/// Produces an arbitrary number of values of type `T`, then proceeds according to `S`. Its dual
/// is [`Enqueue<T, Dual<S>>`].
///
/// Use [`pop`](Self::pop) to obtain the next item of type `T` from the queue (if there is any),
/// or the continuation `S` if all the values have already been popped. Use [`fold`](Self::fold) or
/// [`for_each`](Self::for_each) to process the values more ergonomically. If the continuation is
/// `()` (the empty session), use [`fold1`](Self::fold1) or [`for_each1`](Self::for_each1).
#[must_use]
pub struct Dequeue<T, S: Session = ()> {
    deq: Recv<Queue<T, S>>,
}

/// Accepts an arbitrary number of values of type `T`, then proceeds according to `S`. Its dual
/// is [`Dequeue<T, Dual<S>>`].
///
/// Use [`push`](Self::push) to send a value over the queue. To stop sending values and obtain the
/// continuaiton `S`, use [`close`](Self::close), or [`close1`](Self::close1) if `S` is `()` (the
/// empty session).
#[must_use]
pub struct Enqueue<T, S: Session = ()> {
    enq: Send<Queue<T, S::Dual>>,
}

/// The result of [`Dequeue::pop`].
pub enum Queue<T, S: Session = ()> {
    Item(T, Dequeue<T, S>),
    Closed(S),
}

impl<T, S: Session> Session for Dequeue<T, S>
where
    T: marker::Send + 'static,
{
    type Dual = Enqueue<T, S::Dual>;

    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        Self {
            deq: Recv::fork_sync(|send| f(Enqueue { enq: send })),
        }
    }

    fn link(self, dual: Self::Dual) {
        self.deq.link(dual.enq)
    }
}

impl<T, S: Session> Session for Enqueue<T, S>
where
    T: marker::Send + 'static,
{
    type Dual = Dequeue<T, S::Dual>;

    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        Self {
            enq: Send::fork_sync(|recv| f(Dequeue { deq: recv })),
        }
    }

    fn link(self, dual: Self::Dual) {
        self.enq.link(dual.deq)
    }
}

impl<T, S: Session> Dequeue<T, S>
where
    T: marker::Send + 'static,
{
    /// Waits to receive the next item of type `T` pushed in the queue, or the continuation `S`
    /// if the queue has been closed.
    #[must_use]
    pub async fn pop(self) -> Queue<T, S> {
        self.deq.recv1().await
    }

    /// Accumulates all the items from the queue into a final result according to the provided
    /// asynchronous function and the initial value. Returns the final result along with the
    /// continuation `S`.
    #[must_use]
    pub async fn fold<A, F>(mut self, init: A, mut f: impl FnMut(A, T) -> F) -> (A, S)
    where
        F: Future<Output = A>,
    {
        let mut accum = init;
        loop {
            match self.pop().await {
                Queue::Item(item, rest) => {
                    accum = f(accum, item).await;
                    self = rest;
                }
                Queue::Closed(session) => return (accum, session),
            }
        }
    }

    /// Runs the provided asynchronous function for each item from the queue. Next iteration
    /// does not start before the previous one finishes. Returns the continuation `S`.
    #[must_use]
    pub async fn for_each<F>(self, mut f: impl FnMut(T) -> F) -> S
    where
        F: Future<Output = ()>,
    {
        self.fold((), |(), item| f(item)).await.1
    }
}

impl<T> Dequeue<T, ()>
where
    T: marker::Send + 'static,
{
    /// Accumulates all the items from the queue into a final result according to the provided
    /// asynchronous function and the initial value. Returns the final result.
    pub async fn fold1<A, F>(self, init: A, f: impl FnMut(A, T) -> F) -> A
    where
        F: Future<Output = A>,
    {
        self.fold(init, f).await.0
    }

    /// Runs the provided asynchronous function for each item from the queue. Next iteration
    /// does not start before the previous one finishes.
    pub async fn for_each1<F>(self, f: impl FnMut(T) -> F)
    where
        F: Future<Output = ()>,
    {
        self.for_each(f).await
    }
}

impl<T, S: Session> Enqueue<T, S>
where
    T: marker::Send + 'static,
{
    /// Closes the queue, signaling to the other side that no more items will be pushed. Returns
    /// the continuation `S`.
    #[must_use]
    pub fn close(self) -> S {
        S::fork_sync(|dual| self.enq.send1(Queue::Closed(dual)))
    }

    /// Pushes a value of type `T` into the queue. The items will be received in the same order
    /// as they were pushed.
    pub fn push(self, item: T) -> Self {
        Self::fork_sync(|dual| self.enq.send1(Queue::Item(item, dual)))
    }
}

impl<T> Enqueue<T, ()>
where
    T: marker::Send + 'static,
{
    /// Closes the queue, signaling to the other side that no more items will be pushed.
    pub fn close1(self) {
        self.close()
    }
}
