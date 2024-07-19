use std::marker;

use futures::Future;

use super::{Recv, Send, Session};

#[must_use]
pub struct Dequeue<T, S: Session = ()> {
    deq: Recv<Queue<T, S>>,
}

#[must_use]
pub struct Enqueue<T, S: Session = ()> {
    enq: Send<Queue<T, S::Dual>>,
}

pub enum Queue<T, S: Session = ()> {
    Pop(T, Dequeue<T, S>),
    Closed(S),
}

impl<T, S: Session> Session for Dequeue<T, S>
where T: marker::Send + 'static
{
    type Dual = Enqueue<T, S::Dual>;

    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        Self { deq: Recv::fork_sync(|send| f(Enqueue { enq: send })) }
    }

    fn link(self, dual: Self::Dual) {
        self.deq.link(dual.enq)
    }
}

impl<T, S: Session> Session for Enqueue<T, S>
where T: marker::Send + 'static
{
    type Dual = Dequeue<T, S::Dual>;

    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        Self { enq: Send::fork_sync(|recv| f(Dequeue { deq: recv })) }
    }

    fn link(self, dual: Self::Dual) {
        self.enq.link(dual.deq)
    }
}

impl<T, S: Session> Dequeue<T, S>
where T: marker::Send + 'static
{
    #[must_use]
    pub async fn pop(self) -> Queue<T, S> {
        self.deq.recv1().await
    }

    #[must_use]
    pub async fn fold<A, F>(mut self, init: A, mut f: impl FnMut(A, T) -> F) -> (A, S)
    where F: Future<Output = A>
    {
        let mut accum = init;
        loop {
            match self.pop().await {
                Queue::Pop(item, rest) => {
                    accum = f(accum, item).await;
                    self = rest;
                }
                Queue::Closed(session) => return (accum, session),
            }
        }
    }

    #[must_use]
    pub async fn for_each<F>(self, mut f: impl FnMut(T) -> F) -> S
    where F: Future<Output = ()>
    {
        self.fold((), |(), item| f(item)).await.1
    }
}

impl<T> Dequeue<T, ()>
where T: marker::Send + 'static
{
    pub async fn fold1<A, F>(self, init: A, f: impl FnMut(A, T) -> F) -> A
    where F: Future<Output = A>
    {
        self.fold(init, f).await.0
    }

    pub async fn for_each1<F>(self, f: impl FnMut(T) -> F)
    where F: Future<Output = ()>
    {
        self.for_each(f).await
    }
}

impl<T, S: Session> Enqueue<T, S>
where T: marker::Send + 'static
{
    #[must_use]
    pub fn close(self) -> S {
        S::fork_sync(|dual| self.enq.send1(Queue::Closed(dual)))
    }

    #[must_use]
    pub fn push(self, item: T) -> Self {
        Self::fork_sync(|dual| self.enq.send1(Queue::Pop(item, dual)))
    }
}

impl<T> Enqueue<T, ()>
where T: marker::Send + 'static
{
    pub fn close1(self) {
        self.close()
    }
}
