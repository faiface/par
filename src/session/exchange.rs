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
//! Sending a value is always non-blocking. Only receiving needs to block
//! (asynchronously) until a value has been sent from the other side. This means
//! it's possible to perform multiple sends without waiting for the recipients to
//! be ready.
//!
//! # Correspondence to linear logic
//!
//! [`Recv`] and [`Send`] directly correspond to the multiplicative connectives of
//! linear logic. Specifically:
//!
//! - `Recv<A, B>` is A ⊗ B
//! - `Send<A, B>` is A<sup>⊥</sup> ⅋ B

use std::{marker, pin::Pin};

use futures::{channel::oneshot, Future};

use super::Session;

#[must_use]
pub struct Recv<T, S: Session = ()> {
    p: Producer<Exchange<T, S>>,
}

#[must_use]
pub struct Send<T, S: Session = ()> {
    c: Consumer<Exchange<T, S::Dual>>,
}

type Producer<T> = Pin<Box<dyn Future<Output = T> + marker::Send + 'static>>;
type Consumer<T> = Box<dyn FnOnce(T) + marker::Send + 'static>;

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
        (dual.c)(Exchange::Link(self))
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
        (self.c)(Exchange::Link(dual))
    }
}

fn endpoints<T, S: Session>() -> (Recv<T, S>, Send<T, S::Dual>)
where
    T: marker::Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let recv = Recv {
        p: Box::pin(async { rx.await.expect("sender dropped") }),
    };
    let send = Send {
        c: Box::new(|x| tx.send(x).ok().expect("receiver dropped")),
    };
    (recv, send)
}

impl<T, S: Session> Recv<T, S>
where
    T: marker::Send + 'static,
{
    #[must_use]
    pub async fn recv(mut self) -> (T, S) {
        loop {
            match self.p.await {
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
        S::fork_sync(|dual| (self.c)(Exchange::Send((value, dual))))
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
    pub fn choose<S: Session>(self, choice: fn(S) -> T) -> S::Dual {
        //TODO: simplify?
        Send {
            c: Box::new(move |x| {
                (self.c)(match x {
                    Exchange::Send((session, ())) => Exchange::Send((choice(session), ())),
                    Exchange::Link(link) => Exchange::Link(Recv {
                        p: Box::pin(
                            async move { Exchange::Send((choice(link.recv1().await), ())) },
                        ),
                    }),
                })
            }),
        }
        .handle()
    }
}

impl<S: Session> Send<S, ()> {
    #[must_use]
    pub fn handle(self) -> S::Dual {
        S::Dual::fork_sync(|session| self.send1(session))
    }
}
