//! Session types are used to model communication protocols for message passing
//! concurrent systems and type-check their implementations, guaranteeing:
//!
//! - **Protocol adherence** -- Expectations delivered, obligations fulfilled.
//! - **Deadlock freedom** -- Cyclic communication is statically ruled out.
//!
//! Complex concurrent systems can be modelled and implemented with confidence, as
//! any type-checked program is guaranteed to adhere to its protocol, and avoid any
//! deadlocks (and livelocks). Message passing itself ensures natural synchronization.
//!
//! It is not always easy to design sound concurrent systems without extensive testing
//! and analysis. Concurrency inherently multiplies the number of ways programs can
//! behave. Session types can provide guidance in designing such systems, by enforcing
//! that programs handle all possibilities. Situations unaccounted for in a protocol
//! described by a session type will be discovered when trying to implement it.*
//!
//! Lastly, session types give names to concurrent concepts and patterns, which
//! enables high levels of abstraction and composability. That makes it easier to
//! reason and talk about large concurrent systems.
//!
//! The particular flavor of session types presented here is a full implementation
//! of propositional linear logic. However, no knowledge of linear logic is required to use
//! or understand this library.
//!
//! *_This is achieved by the same principles as those applied to Rust's error handling._
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
//! use par::runtimes::tokio::fork;
//!
//! let sender: Send<i64> = fork(|receiver: Recv<i64>| async {
//!     assert_eq!(receiver.recv1().await, 7);
//! });
//! sender.send1(7);
//! ```
//!
//! Now we will take a look at three basic ways to compose sessions:
//! **sequencing**, **branching**, and **recursion**. These, together with
//! [Recv](exchange::Recv) and [Send](exchange::Send), are enough to construct
//! any complex session types (within the possibilities of the present framework). The
//! other modules, [queue] and [pool], merely standardize some (very useful) patterns.
//!
//! # Sequencing
//!
//! The two session types above, [Recv](exchange::Recv) and [Send](exchange::Send)
//! actually take a second generic parameter: the continuation of the session. This
//! continuation defaults to `()` if we leave it out.
//!
//! ```
//! pub struct Recv<T, S: Session = ()> { /* private fields */ }
//! pub struct Send<T, S: Session = ()> { /* private fields */ }
//! ```
//!
//! The unit type `()` implements [Session] and represents an empty, finished session.
//! It's self-dual, its dual is itself.*
//!
//! We can choose different continuations to make sequential sessions. For example:
//!
//! ```
//! use par::{exchange::{Recv, Send}, Session};
//!
//! type Calculator = Send<i64, Send<Op, Send<i64, Recv<i64>>>>;
//! enum Op { Plus, Times }
//! ```
//!
//! Here we have a session which is sending two numbers and an operator, and finally
//! receives a number back, presumably the result.
//!
//! To get a dual of a sequential session, we also have to dualize the continuation.
//! In the end, we just flip every [Recv](exchange::Recv) to [Send](exchange::Send)
//! and vice versa.
//!
//! The dual of the `Calculator` session is then
//! `Recv<i64, Recv<Op, Recv<i64, Send<i64>>>>`.
//!
//! Here's one possible implementation:
//!
//! ```
//! use par::{runtimes::tokio::fork, Dual};
//!
//! type User = Dual<Calculator>;  // Recv<i64, Recv<Op, Recv<i64, Send<i64>>>>
//!
//! fn start_calculator() -> Calculator {
//!     fork(|user: User| async {
//!         let (x, user) = user.recv().await;
//!         let (op, user) = user.recv().await;
//!         let (y, user) = user.recv().await;
//!         let result = match op {
//!             Op::Plus => x + y,
//!             Op::Times => x * y,
//!         };
//!         user.send1(result);
//!     })
//! }
//!
//! async fn calculate() {
//!     let sum = start_calculator()
//!         .send(3)
//!         .send(Op::Plus)
//!         .send(4)
//!         .recv1()
//!         .await;
//!
//!     let product = start_calculator()
//!         .send(3)
//!         .send(Op::Times)
//!         .send(4)
//!         .recv1()
//!         .await;
//!
//!     assert_eq!(sum, 7);
//!     assert_eq!(product, 12);
//! }
//! ```
//!
//! The type [`Dual<T>`] is just a convenient alias for `<T as Session>::Dual`.
//!
//! We use four different methods to communicate in the session:
//!
//! - For [Recv](exchange::Recv) it's [.recv()](exchange::Recv::recv) and
//!   [.recv1()](exchange::Recv::recv1), which need to be `.await`-ed.
//! - For [Send](exchange::Send) it's [.send()](exchange::Send::send) and
//!   [.send1()](exchange::Send::send1), which are not `async`.
//!
//! The difference between [.recv()](exchange::Recv::recv) and
//! [.recv1()](exchange::Recv::recv1), and between [.send()](exchange::Send::send)
//! and [.send1()](exchange::Send::send1) is about the continuation.
//! The versions ending with `1`, [.recv1()](exchange::Recv::recv1) and
//! [.send1()](exchange::Send::send1) can only be used if the continuation is `()`.
//! Unlike their general versions, [.recv()](exchange::Recv::recv) and
//! [.send()](exchange::Send::send), they don't return a continuation, and are
//! not marked as `#[must_use]`.**
//!
//! The general [.recv()](exchange::Recv::recv) and [.send()](exchange::Send::send)
//! can always be used instead of [.recv1()](exchange::Recv::recv1) and
//! [.send1()](exchange::Send::send1), but then we have to deal with the returned
//! `()`. The difference is just about convenience.
//!
//! *_In the standard presentation of linear logic, the unit type **1** is not self-dual,
//! **1<sup>⊥</sup> = ⊥**. However, it becomes self-dual if we include the so-called MIX
//! rule, which states that **A ⊗ B** can be converted to **A ⅋ B**. This doesn't do any harm
//! to the logic (it just enables disconnected processes), and makes it easier to
//! program with._
//!
//! **_The methods returning a continuation, [.recv()](exchange::Recv::recv) and
//! [.send()](exchange::Send::send), are marked as `#[must_use]` because sessions
//! must not be dropped._

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
