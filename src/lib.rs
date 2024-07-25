//! Session types are used to model communication protocols for message passing
//! concurrent systems and type-check their implementations, guaranteeing:
//!
//! - **Protocol adherence** -- Expectations delivered, obligations fulfilled.
//! - **Deadlock freedom** -- The two ends of each exchange are always isolated from each other.
//!
//! Complex concurrent systems can be modelled and implemented with confidence, as
//! any type-checked program is guaranteed to adhere to its protocol, and avoid any
//! deadlocks. Message passing itself ensures natural synchronization.
//!
//! It is not always easy to design sound concurrent systems without extensive testing
//! and analysis. Concurrency inherently multiplies the number of ways programs can
//! behave. Session types can provide guidance in designing such systems, by enforcing
//! that programs handle all possibilities. Situations unaccounted for in protocols
//! described by session types are exposed before runtime, as it will not be possible
//! to implement them.
//!
//! Lastly, session types give names to concurrent concepts and patterns, which
//! enables high levels of abstraction and composability. That makes it easier to
//! reason and talk about large concurrent systems.
//!
//! _The particular flavor of session types presented here is a full implementation
//! of propositional linear logic. However, no knowledge of linear logic is required to use
//! or understand this library._
//!
//! # Forking
//!
//! Communication involves two opposite points of view. For example, an exchange
//! of a message is seen as _sending_ on one side, and _receiving_ on the other.
//!
//! A type implementing the [Session] trait can be thought of as a handle to one of the
//! two viewpoints. Its associated type [Dual](Session::Dual) represents the other
//! side, cleverly restricted as [`Session<Dual = Self>`](Session::Dual), so
//! that the two types form a dual pair.
//!
//! For example, [`Recv<T>`](exchange::Recv) and [`Send<T>`](exchange::Send) from
//! the [exchange] module are dual to one another.
//!
//! A session handle can only be created together with its dual (to ensure protocol adherence),
//! in two independent scopes (to prevent deadlocks). This moment of creation is called
//! *forking*. The [Session] trait has its [fork_sync](Session::fork_sync) method for this
//! purpose.
//!
//! ```
//! use par::{exchange::{Recv, Send}, Session};
//!
//! async fn forking() {
//!     let receiver: Recv<i64> = Recv::fork_sync(|sender: Send<i64>| {
//!         // scope of Send<i64>
//!         sender.send1(7);
//!     });
//!     // scope of Recv<i64>
//!     assert_eq!(receiver.recv1().await, 7);
//! }
//! ```
//!
//! The function passed to [fork_sync](Session::fork_sync) is not asynchronous and is
//! fully executed before [fork_sync](Session::fork_sync) returns. Usually, we want
//! the two scopes to run concurrently, but that is dependent on the choice of an
//! `async/await` runtime. In order to be runtime-agnostic, the [Session] trait itself
//! only provides synchronous forking.
//!
//! To enable concurrency, we spawn a coroutine/thread inside the synchronous function.
//! For example, with Tokio:
//!
//! ```
//! let sender: Send<i64> = Send::fork_sync(|receiver: Recv<i64>| {
//!     drop(tokio::spawn(async {
//!         assert_eq!(receiver.recv1().await, 7);
//!     }))
//! });
//! sender.send1(7);
//! ```
//!
//! This crate provides utility modules for forking using popular runtimes. With these,
//! the above can be reduced to:
//!
//! ```
//! use par::tokio::fork;
//!
//! let sender: Send<i64> = fork(|receiver: Recv<i64>| async {
//!     assert_eq!(receiver.recv1().await, 7);
//! });
//! sender.send1(7);
//! ```

pub mod exchange;
pub mod pool;
pub mod queue;
pub mod runtimes;

pub trait Session: Send + 'static {
    type Dual: Session<Dual = Self>;

    #[must_use]
    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self;
    fn link(self, dual: Self::Dual);
}

pub type Dual<S> = <S as Session>::Dual;

impl Session for () {
    type Dual = ();
    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        f(())
    }
    fn link(self, (): Self::Dual) {}
}
