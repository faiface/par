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
//! other modules, [queue] and [server], merely standardize some (very useful) patterns.
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
//! It's self-dual, its dual is `()`.*
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
//! The versions ending with 1, [.recv1()](exchange::Recv::recv1) and
//! [.send1()](exchange::Send::send1), can only be used if the continuation is `()`.
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
//!
//! # Branching
//!
//! Now that we know how to move forward, it's time to take a turn. Say we want to
//! describe a communication between an ATM and a client interacting with its buttons.
//! To make it simple, the interaction goes like this:
//!
//! 1. Client inserts a bank card.
//! 2. If the card number is not valid, ATM rejects it and the session ends.
//! 3. Otherwise, the ATM presents the client with two buttons:
//!    - **Check balance:** ATM shows the balance on the account and the session ends.
//!    - **Withdraw cash:** After pressing this button, the client enters the desired amount to withdraw.
//!      - If the amount is above the card's balance, ATM rejects and the session ends.
//!      - Otherwise all is good, the ATM outputs the desired amount and the session ends, too.
//!
//! To model such branching interaction, no additional dedicated types are provided by this crate.
//! Instead, we make use of Rust's native enums, and the ability to send and receive not only values,
//! but session as well.
//!
//! We start backwards, by modeling the interaction after an operation is selected:
//!
//! ```
//! use par::exchange::{Recv, Send};
//!
//! struct Amount(i64);
//! struct Money(i64);
//! struct InsufficientFunds;
//!
//! enum Operation {
//!     CheckBalance(Send<Amount>),
//!     Withdraw(Recv<Amount, Send<Result<Money, InsufficientFunds>>>),
//! }
//! ```
//!
//! The helper types -- `Amount`, `Money`, and `InsufficientFunds` -- are just to aid readability.
//!
//! When branching, the first thing to decide is who is choosing, and who is offering a choice. In this
//! case, the client is choosing from two options offered by the ATM. In other words, the client will
//! send a selected `Operation` to the ATM, which receives it.
//!
//! That's why the sessions exchanged are described from the ATM's point of view:
//! - `Operation::CheckBalance` proceeds to send the account's balance to the client.
//! - `Operation::Withdraw` starts by receiving an amount requested by the client, then goes on to
//!   either send cash to the client, or reject due to insufficient funds.
//!
//! Note, that there are already (kind of) two branching points above. One is between the two buttons,
//! the second one is chosen by the ATM to accept or reject the requested amount. This second choice
//! is only a "kind of" branching because no sessions are exchanged. But, an equivalent way to encode
//! it would be to include a reception of the money on the client's side:
//! `...Result<Recv<Money>, InsufficientFunds>...`.
//!
//! The beginning of the interaction then involves sending a selected `Operation` to the ATM, after
//! validating the account's number:
//!
//! ```
//! struct Account(String);
//! struct InvalidAccount;
//!
//! type ATM = Send<Account, Recv<Result<Send<Operation>, InvalidAccount>>>;
//! ```
//!
//! Here the session is described from the client's point of view. That's arbitrary choice -- the ATM's point
//! of view is simply the dual of the above:
//!
//! ```
//! use par::Dual;
//!
//! type Client = Dual<ATM>;  // Recv<Account, Send<Result<Send<Operation>, InvalidAccount>>>
//! ```
//!
//! ## [`Send::choose`](exchange::Send::choose)
//!
//! Being able to model a session by freely using custom (`Operation`) or standard ([`Result`]) enums
//! is certainly expressive, but how ergonomic is it to use? Turns out it can be quite ergonomic thanks to
//! the [`.choose()`](exchange::Send::choose) method on [`Send`](exchange::Send).
//!
//! Let's implement a client that checks the balance on their account. Here's a function that, given an
//! account number, starts a `Client` session which does exactly that:
//!
//! ```
//! use par::runtimes::tokio::fork;
//!
//! fn check_balance(number: String) -> Client {
//!     fork(|atm: ATM| async move {
//!         let atm = atm.send(Account(number.clone()));
//!         let Ok(atm) = atm.recv1().await else {
//!             return println!("invalid account: {}", number);
//!         };
//!         let Amount(funds) = atm.choose(Operation::CheckBalance).recv1().await;
//!         println!("{} has {}", number, funds);
//!     })
//! }
//! ```
//!
//! After sending the account number and receiving a positive response from the ATM, the client is
//! presented with a choice of the operation: _check balance_ or _withdraw money_.
//!
//! ```
//! let Amount(funds) = atm.choose(Operation::CheckBalance).recv1().await;
//! ```
//!
//! At this point, the type of `atm` is `Send<Operation>`. But instead of calling [`.send1()`](exchange::Send::send1),
//! we [choose](exchange::Send::choose) `Operation::CheckBalance`, then receive the answer from the ATM. What is
//! going on?
//!
//! Without using [`.choose()`](exchange::Send::choose), there are two ways to accomplish the same.
//!
//! 1. The **inside** way:
//!
//!    ```
//!    atm.send1(Operation::CheckBalance(fork(|atm: Recv<Amount>| async move {
//!        let Amount(funds) = atm.recv1().await;
//!        println!("{} has {}", number, funds);
//!    })));
//!    ```
//!
//! 2. The **outside** way:
//!
//!    ```
//!    let atm: Recv<Amount> = fork(|client: Send<Amount>| async move {
//!        atm.send1(Operation::CheckBalance(client));
//!    });
//!    let Amount(funds) = atm.recv1().await;
//!    println!("{} has {}", number, funds);
//!    ```
//!
//!    Which can also be written with [`fork_sync`](Session::fork_sync)!
//!
//!    ```
//!    let atm = <Recv<Amount>>::fork_sync(|client: Send<Amount>| {
//!        atm.send1(Operation::CheckBalance(client))
//!    });
//!    let Amount(funds) = atm.recv1().await;
//!    println!("{} has {}", number, funds);
//!    ```
//!
//! The inside way introduces nesting of the follow-up code which the outside way avoids. Since
//! avoiding nesting is beneficial enough to warrant (validly) the whole async/await paradigm
//! (replacing nested callbacks), the **outside** way is superior.
//!
//! In short, all [`.choose()`](exchange::Send::choose) does is codify this outside form into a method.
//! With any `Enum` and its `Enum::Variant`,
//!
//! ```
//! let session = Session::fork_sync(|dual| session.send1(Enum::Variant(dual)));
//! ```
//!
//! becomes
//!
//! ```
//! let session = session.choose(Enum::Variant);
//! ```
//!
//! Without any additional explanation, here's a possible implementation of a client withdrawing money.
//!
//! ```
//! fn withdraw(number: String, Amount(requested): Amount) -> Client {
//!     fork(|atm: ATM| async move {
//!         let Ok(atm) = atm.send(Account(number.clone())).recv1().await else {
//!             return println!("invalid account: {}", number);
//!         };
//!         let response = atm
//!             .choose(Operation::Withdraw)
//!             .send(Amount(requested))
//!             .recv1()
//!             .await;
//!         match response {
//!             Ok(Money(withdrawn)) => println!("{} withdrawn from {}", withdrawn, number),
//!             Err(InsufficientFunds) => println!(
//!                 "{} has insufficient funds to withdraw {}",
//!                 number, requested
//!             ),
//!         }
//!     })
//! }
//! ```
//!
//! # Recursion

pub mod exchange;
pub mod server;
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
