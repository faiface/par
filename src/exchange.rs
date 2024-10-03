//! Exchange a single value (possibly another session), then proceed according to
//! a continuation session. The two sides, receiving and sending, are [`Recv`] and [`Send`],
//! respectively.
//!
//! Sending is always non-blocking. Only receiving needs to block (asynchronously) until
//! a value is supplied by the other side. It's possible to perform multiple sends without waiting
//! for the recipients to be ready.
//!
//! ## Correspondence to linear logic
//!
//! [`Recv`] and [`Send`] directly correspond to the multiplicative connectives of linear logic.
//!
//! - `Recv<A, B>` is **A ⊗ B**
//! - `Send<A, B>` is **A<sup>⊥</sup> ⅋ B**
//!
//! Their unary versions composed with enums correspond to the additive connectives of linear logic.
//! For example, with `Result`:
//!
//! - `Recv<Result<A, B>>` is **A ⊕ B**
//! - `Send<Result<A, B>>` is **A<sup>⊥</sup> & B<sup>⊥</sup>**

use super::Session;
use futures::channel::oneshot;
use std::marker;

/// Supplies a value of type `T`, then proceeds according to `S`. Its dual is [`Send<T, Dual<S>>`].
///
/// Use [`recv`](Self::recv) to obtain the supplied value along with the continuation `S`.
/// If the continuation is `()` (the empty session), use [`recv1`](Self::recv1) to obtain `T`
/// and discard the continuation.
///
/// ## Correspondence to linear logic
///
/// - `Recv<A, B>` is **A ⊗ B**
/// - `Recv<Result<A, B>>` is **A ⊕ B**
#[must_use]
pub struct Recv<T, S: Session = ()> {
    rx: oneshot::Receiver<Exchange<T, S>>,
}

/// Consumes a value of type `T`, then proceeds according to `S`. Its dual is [`Recv<T, Dual<S>>`].
///
/// Use [`send`](Self::send) to supply the requested value and obtain the continuation `S`.
/// If the continuation is `()` (the empty session), use [`send1`](Self::send1) to discard the
/// continuation.
///
/// ```
/// let sequence: Send<i64, Send<i64>>;
/// sequence.send(5).send1(7);
/// ```
///
/// If `T` is an `enum` holding other session types (and `S` is `()`), use [`choose`](Self::choose) to
/// pick a branch and directly handle the [dual](crate::Session::Dual) of the supplied session.
///
/// ```
/// let branching: Send<Result<Recv<i64>, ()>>;
/// branching.choose(Ok).send1(7);
/// ```
///
/// ## Correspondence to linear logic
///
/// - `Send<A, B>` is **A<sup>⊥</sup> ⅋ B**
/// - `Send<Result<A, B>>` is **A<sup>⊥</sup> & B<sup>⊥</sup>**
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
    /// Waits to obtain a value of type `T` along with the continuation `S`.
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
    /// Waits to obtain a value of type `T`, and discards the empty continuation.
    pub async fn recv1(self) -> T {
        self.recv().await.0
    }
}

impl<T, S: Session> Send<T, S>
where
    T: marker::Send + 'static,
{
    /// Supplies a value of type `T` and obtains the continuation `S`.
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
    /// Supplies a value of type `T`, and discards the empty continuation.
    pub fn send1(self, value: T) {
        self.send(value)
    }

    /// If the expected value is an `enum` holding sessions, chooses a branch from the `enum`'s variants,
    /// and directly obtains the [dual](crate::Session::Dual) of the supplied session.
    ///
    /// ```
    /// let branching: Send<Result<Recv<i64>, ()>>;
    /// branching.choose(Ok).send1(7);
    /// ```
    #[must_use]
    pub fn choose<S: Session>(self, choice: impl FnOnce(S) -> T) -> S::Dual {
        S::Dual::fork_sync(|session| self.send1(choice(session)))
    }
}

impl<S: Session> Send<S, ()> {
    /// If the expected value is a session, supplies it and directly obtains its [dual](crate::Session::Dual).
    ///
    /// ```
    /// let needs_session: Send<Recv<i64>>;
    /// needs_session.handle().send1(7);
    /// ```
    #[must_use]
    pub fn handle(self) -> S::Dual {
        S::Dual::fork_sync(|session| self.send1(session))
    }
}
