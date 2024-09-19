//! Sessions that first exchange a single value (possibly also a session) between
//! their two counterparts, then proceed according to another specified session. The
//! two sides, receiving and sending, are [`Recv`] and [`Send`] respectively.
//!
//! A session exchanging a value of type `T` and continuing according to a session
//! `S` will be operated via two handles: [`Recv<T, S>`] in one coroutine, cooperating
//! with a [`Send<T, Dual<S>>`] in another.
//!
//! After exchanging a `T`, the two counterparts obtain handles for `S` and
//! [`Dual<S>`](super::Dual) respectively, continuing their cooperation according
//! to the session `S`.
//!
//! # Blocking and `.await`
//!
//! Sending is always non-blocking. Only receiving needs to block (asynchronously) until
//! a value has been sent from the other side. This means it's possible to perform multiple
//! sends without waiting for the recipients to be ready.
//!
//! # Correspondence to linear logic
//!
//! [`Recv`] and [`Send`] directly correspond to the multiplicative connectives of
//! linear logic. Specifically:
//!
//! - `Recv<A, B>` is **A ⊗ B**
//! - `Send<A, B>` is **A<sup>⊥</sup> ⅋ B**
//!
//! And, their unary versions composed with enums correspond to the additive connectives
//! of linear logic. Using the enum `Result`, we have:
//!
//! - `Recv<Result<A, B>>` is **A ⊕ B**
//! - `Send<Result<A, B>>` is **A<sup>⊥</sup> & B<sup>⊥</sup>**

use super::Session;
use futures::channel::oneshot;
use std::marker;

#[must_use]
pub struct Recv<T, S: Session = ()> {
    rx: oneshot::Receiver<Exchange<T, S>>,
}

#[must_use]
pub struct Send<T, S: Session = ()> {
    tx: oneshot::Sender<Exchange<T, S::Dual>>,
}

enum Exchange<T, S: Session> {
    Send((T, S)),
    Link(Recv<T, S>),
}

impl<T, S: Session> Session for Recv<T, S>
where
    T: marker::Send + 'static,
{
    type Dual = Send<T, S::Dual>;

    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        let (recv, send) = endpoints();
        f(send);
        recv
    }

    fn link(self, dual: Self::Dual) {
        dual.link(self)
    }
}

impl<T, S: Session> Session for Send<T, S>
where
    T: marker::Send + 'static,
{
    type Dual = Recv<T, S::Dual>;

    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        let (recv, send) = endpoints();
        f(recv);
        send
    }

    fn link(self, dual: Self::Dual) {
        self.tx
            .send(Exchange::Link(dual))
            .ok()
            .expect("receiver dropped")
    }
}

fn endpoints<T, S: Session>() -> (Recv<T, S>, Send<T, S::Dual>)
where
    T: marker::Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    (Recv { rx }, Send { tx })
}

impl<T, S: Session> Recv<T, S>
where
    T: marker::Send + 'static,
{
    #[must_use]
    pub async fn recv(mut self) -> (T, S) {
        loop {
            match self.rx.await.expect("sender dropped") {
                Exchange::Send(x) => break x,
                Exchange::Link(r) => self = r,
            }
        }
    }
}

impl<T> Recv<T, ()>
where
    T: marker::Send + 'static,
{
    pub async fn recv1(self) -> T {
        self.recv().await.0
    }
}

impl<T, S: Session> Send<T, S>
where
    T: marker::Send + 'static,
{
    #[must_use]
    pub fn send(self, value: T) -> S {
        S::fork_sync(|dual| {
            self.tx
                .send(Exchange::Send((value, dual)))
                .ok()
                .expect("receiver dropped")
        })
    }
}

impl<T> Send<T, ()>
where
    T: marker::Send + 'static,
{
    pub fn send1(self, value: T) {
        self.send(value)
    }

    #[must_use]
    pub fn choose<S: Session>(self, choice: impl FnOnce(S) -> T) -> S::Dual {
        S::Dual::fork_sync(|session| self.send1(choice(session)))
    }
}

impl<S: Session> Send<S, ()> {
    #[must_use]
    pub fn handle(self) -> S::Dual {
        S::Dual::fork_sync(|session| self.send1(session))
    }
}
