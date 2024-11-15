//! **What's a session type, anyway?** It's a description an entire external behavior
//! of a concurrent, message-passing process. From the first message, through every
//! possible path and interaction that can be made with it, to all the ways it can
//! finish.
//!
//! When implementing a concurrent program according to a session type, the type tells
//! what can happen at any point in the program. When we have to send a message, when to
//! select a path to continue, when to wait for someone else to make a choice and adapt.
//!
//! Crucially, the types are designed to provide some useful guarantees:
//!
//! - **Protocol adherence** -- Expectations delivered, obligations fulfilled.
//! - **Deadlock freedom** -- Cyclic communication is statically ruled out.
//!
//! _Protocol adherence_ means that when interacting with a process described by a session
//! type, we can be sure (unless it crashes) that it will behave according to the protocol
//! specified by its type. There will no unexpected messages, nor forgotten obligations.
//! Just like we can rely on a function to return a string if it says so, we can rely on
//! a process to send a string if its specified anywhere within its session type.
//!
//! _Deadlock freedom_ means that deadlocks can't happen, without dirty tricks anyway. It is
//! achieved by imposing a certain structure on how processes are connected. It can take
//! a bit of getting used to, but it becomes very important when implementing very complex
//! concurrent systems.
//!
//! > ❓ Reading this, one may easily think, _"I don't see deadlocks happen in practice..."_,
//! and that's a valid objection! But it arises from our concurrent systems not being very
//! complex due to a lack of tools and types to design and implement them reliably.
//! At high levels of complexity, deadlocks become an issue, and having them ruled out
//! proves crucial.
//!
//! Using session types, complex concurrent systems can be modelled and implemented with confidence,
//! as any type-checked program is guaranteed to adhere to its protocol, and avoid any deadlocks.
//! Message passing itself ensures natural synchronization.
//!
//! Lastly, session types give names to concurrent concepts and patterns, which
//! enables high levels of abstraction and composability. That makes it easier to
//! reason and talk about large concurrent systems.
//!
//! > 📚 The particular flavor of session types presented here is a full implementation
//! of propositional linear logic. However, no knowledge of linear logic is required to use
//! or understand this library.
//!
//! # Forking
//!
//! Communication involves two opposing points of view. For example, an exchange
//! of a message is seen as _sending_ on one side, and _receiving_ on the other. The idea
//! extends to larger communication protocols. A turn-based game between two players looks
//! like _"my move, your move"_ by one player, and _"your move, my move"_ by their opponent.
//! Other examples are _"a player and the environment"_, _"a customer and a shop assistant"_,
//! _"a server and a client"_.
//!
//! A type implementing the [`Session`] trait can be thought of as a handle to one of the
//! two viewpoints of a certain communication protocol. Its associated type [Dual](Session::Dual)
//! represents the other side, cleverly restricted as [`Session<Dual = Self>`](Session::Dual),
//! so that the two types form a dual pair. **A concrete [`Session`] and its dual together
//! describe a protocol.**
//!
//! For example, [`Recv<T>`](exchange::Recv) and [`Send<T>`](exchange::Send) from
//! the [exchange] module form such a pair.
//!
//! A session handle can only be created together with its dual, and only in two independent
//! visibility scopes. The former ensures protocol adherence -- the other side is always
//! there --, while the latter prevents deadlocks -- different visibility scopes enable
//! independent progress.
//!
//! This moment of creation is called *forking*. The [Session] trait has its
//! [fork_sync](Session::fork_sync) method for this purpose.
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
//! fully executed before [fork_sync](Session::fork_sync) returns. Many useful combinators
//! do not require `async`: offering only `async` forking would hurt those. Additionally,
//! it enables the core library to be agnostic about the choice of an `async/await` runtime.
//!
//! However, in an application, we usually want the two sides to run concurrently. To do that,
//! we spawn a coroutine/thread inside the synchronous function. For example, with Tokio:
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
//! To make life easier, this crate provides utility modules for forking using popular runtimes.
//! With these, the above can be reduced to:
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
//! > ❗️ Session end-points **must not be dropped.** (TODO: explain...)
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
//! It's self-dual, its dual is `()`.
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
//! - For [Recv](exchange::Recv) it's [`recv`](exchange::Recv::recv) and
//!   [`recv1`](exchange::Recv::recv1), which need to be `.await`-ed.
//! - For [Send](exchange::Send) it's [`send`](exchange::Send::send) and
//!   [`send1`](exchange::Send::send1), which are not `async`.
//!
//! The difference between [`recv`](exchange::Recv::recv) and
//! [`recv1`](exchange::Recv::recv1), and between [`send`](exchange::Send::send)
//! and [`send1`](exchange::Send::send1) is about the continuation.
//! The versions ending with 1, [`recv1`](exchange::Recv::recv1) and
//! [`send1`](exchange::Send::send1), can only be used if the continuation is `()`.
//! Unlike their general versions, [`recv`](exchange::Recv::recv) and
//! [`send`](exchange::Send::send), they don't return a continuation, and are
//! not marked as `#[must_use]`.**
//!
//! The general [`recv`](exchange::Recv::recv) and [`send`](exchange::Send::send)
//! can always be used instead of [`recv1`](exchange::Recv::recv1) and
//! [`send1`](exchange::Send::send1), but then we have to deal with the returned
//! `()`. The difference is just about convenience.
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
//! ## Picking a choice with [`Send::choose`](exchange::Send::choose)
//!
//! Being able to model a session by freely using custom (`Operation`) or standard ([`Result`]) enums
//! is certainly expressive, but how ergonomic is it to use? Turns out it can be quite ergonomic thanks to
//! the [`choose`](exchange::Send::choose) method on [`Send`](exchange::Send).
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
//!             return println!("Invalid account: {}", number);
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
//! At this point, the type of `atm` is `Send<Operation>`. But instead of calling [`send1`](exchange::Send::send1),
//! we [choose](exchange::Send::choose) `Operation::CheckBalance`.
//!
//! Recall that the payload of `Operation::CheckBalance` is `Send<Amount>`. [`Send::choose`](exchange::Send::choose)
//! returns the **dual of that payload!**
//!
//! ```
//! atm.choose(Operation::CheckBalance) // -> Recv<Amount>
//!     .recv1().await;
//! ```
//!
//! That's why we can simply receive the response from the ATM afterwards.
//!
//! **What's going on?**
//!
//! There are two manual ways to accomplish what [`choose`](exchange::Send::choose) does.
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
//! In short, all [`choose`](exchange::Send::choose) does is codify this outside form into a method.
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
//!             return println!("Invalid account: {}", number);
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
//! # Linking
//!
//! In the last example, we defined dual sessions `ATM` and `Client` and implemented two behaviors
//! for `Client`.
//!
//! ```
//! fn check_balance(number: String) -> Client { /* ... */ }
//! fn withdraw(number: String, Amount(requested): Amount) -> Client { /* ... */ }
//! ```
//!
//! For a full program, we're missing the `ATM`'s side. We could just take the returned `Client`s
//! and interact with them directly, but we can also construct an `ATM` separately. Then the question
//! becomes how to **wire them together.**
//!
//! To show that, we first need an `ATM`. For simplicity, we'll be retrieving the accounts from a
//! `HashMap<String, Money>`, without updating their balances upon withdrawals. Implementing that is
//! left as a simple optional exercise for the reader.
//!
//! Understanding the implementation below is not important for this section. All that's important is
//! to know the `ATM` looks in the provided `accounts`, and responds depending on the existence of a
//! requested account and its balance.
//!
//! ```
//! use std::collections::HashMap;
//! use std::sync::Arc;
//!
//! fn boot_atm(accounts: Arc<HashMap<String, Money>>) -> ATM {
//!     fork(|client: Client| async move {
//!         let (Account(number), client) = client.recv().await;
//!         let Some(&Money(funds)) = accounts.get(&number) else {
//!             return client.send1(Err(InvalidAccount));
//!         };
//!         match client.choose(Ok).recv1().await {
//!             Operation::CheckBalance(client) => client.send1(Amount(funds)),
//!             Operation::Withdraw(client) => {
//!                 let (Amount(requested), client) = client.recv().await;
//!                 if funds >= requested {
//!                     client.send1(Ok(Money(requested)));
//!                 } else {
//!                     client.send1(Err(InsufficientFunds));
//!                 }
//!             }
//!         }
//!     })
//! }
//! ```
//!
//! Let's boot the ATM!
//!
//! ```
//! let accounts = Arc::new(HashMap::from([
//!     ("Alice".to_string(), Money(1000)),
//!     ("Bob".to_string(), Money(700)),
//!     ("Cyril".to_string(), Money(5500)),
//! ]));
//!
//! let atm = boot_atm(Arc::clone(&accounts));
//! ```
//!
//! Then start a `Client` session to withdraw some money from Cyril's account.
//!
//! ```
//! let client = withdraw("Cyril".to_string(), Amount(2500));
//! ```
//!
//! The two dual sessions are now up and running. For wiring them together, the [Session] trait
//! provides a useful method: [`Session::link`]!
//!
//! Like [`send`](exchange::Send::send), it is **non-blocking** and **non-async**: it tells the
//! two sessions to talk to each other and immediately proceeds, no `.await` required. Here's what
//! it looks like:
//!
//! ```
//! use par::Session;
//!
//! atm.link(client);
//! ```
//!
//! And the whole program:
//!
//! ```
//! use par::Session;
//!
//! #[tokio::main]
//! async fn main() {
//!     let accounts = Arc::new(HashMap::from([
//!         ("Alice".to_string(), Money(1000)),
//!         ("Bob".to_string(), Money(700)),
//!         ("Cyril".to_string(), Money(5500)),
//!     ]));
//!
//!     let atm = boot_atm(Arc::clone(&accounts));
//!     let client = withdraw("Cyril".to_string(), Amount(2500));
//!
//!     atm.link(client);
//!
//!     // atm and client talk in the background, let them speak
//!     tokio::time::sleep(Duration::from_secs(1)).await;
//! }
//! ```
//!
//! **A caveat of non-blocking functions** like [`send`](exchange::Send::send) or [`link`](Session::link)
//! is we need to add extra synchronization if we need to wait for the outcome. This usually isn't a concern when
//! a program is well intertwined -- which it usually is. In this case, though, we need to wait for the two
//! parties to finish interacting before exiting. We could insert an auxiliary [`Recv`](exchange::Recv), but once
//! again, for simplicity, we just sleep.
//!
//! ```plain
//! 2500 withdrawn from Cyril
//! ```
//!
//! Linking is like a generalized calling of a function. It can be done on a single line, even if it's a little
//! verbose in Rust.
//!
//! ```
//! #[tokio::main]
//! async fn main() {
//!     let accounts = Arc::new(HashMap::from([
//!         ("Alice".to_string(), Money(1000)),
//!         ("Bob".to_string(), Money(700)),
//!         ("Cyril".to_string(), Money(5500)),
//!     ]));
//!
//!     boot_atm(Arc::clone(&accounts)).link(check_balance("Alice".to_string()));
//!     boot_atm(Arc::clone(&accounts)).link(withdraw("Bob".to_string(), Amount(1000)));
//!     boot_atm(Arc::clone(&accounts)).link(withdraw("Dylan".to_string(), Amount(20)));
//!
//!     tokio::time::sleep(Duration::from_secs(1)).await;
//! }
//! ```
//!
//! ```plain
//! Bob has insufficient funds to withdraw 1000
//! Alice has 1000
//! Invalid account: Dylan
//! ```
//!
//! # Recursion
//!
//! In the realm of algebraic types, the basic building blocks of products and sums (`struct`s and `enum`s
//! in Rust) **explode** into lists, maps, stacks, queues, and all kinds of other powerful data structures --
//! **via recursion.** The same happens in the realm of session types: the basic building blocks of **sequencing
//! and branching** make processing pipelines, worker pools, servers, game rule protocols, and so much more, when
//! combined recursively.
//!
//! Now that we've covered those basic building blocks, let's take a look at how to create recursive session
//! types to define complex and intricate communication protocols.
//!
//! To start, we'll stay with two-party protocols, but in the next section, we'll also demonstrate how to
//! construct sessions intertwining **more than two parties** (/ agents / processes / threads).
//!
//! **One of the most common tasks** that involve repeating something an unknown number of times is **processing
//! a stream of incoming data.** The [queue] module implements dedicated session types for this purpose. Before
//! taking a look at it, though, we'll implement such a protocol manually, to see how it's done.
//!
//! **The task is:** There is an incoming stream of integers. Add them all up and report the total sum back.
//!
//! A recursive session protocol is exactly what we need to accomplish this. First it needs to branch on
//!
//! 1. **receiving a number to add**, or
//! 2. **finishing and reporting the total**.
//!
//! Then, in the first case, it needs to go back and repeat, until eventually reaching the second case.
//!
//! Native recursion on types in Rust is all we need:
//!
//! ```
//! enum Counting {
//!     More(Recv<i64, Recv<Counting>>),
//!     Done(Send<i64>),
//! }
//! ```
//!
//! The `enum` defines two variants. On `Counting::More`, the counter receives a number and continues
//! recursively. On `Counting::Done`, it's required to send a number back: the total.
//!
//! A couple of things to note:
//! - `Counting` itself is not a session type, just an `enum`. The two sides of the session will be
//!   using `Recv<Counting>` (from the counter's point of view) and `Send<Counting>`. In more complicated
//!   use-cases, it's recommended to set type aliases for the respective `Recv<...>` and `Send<...>` sides.
//! - No `Box` or `Arc` is required at the recursion point, `Counting` is [`Sized`]. That's because the
//!   memory indirection needed for recursive types is already taken care of by the channels used in
//!   [`Recv`](exchange::Recv)/[`Send`](exchange::Send).
//!
//! While the session type is recursive, its implementation doesn't have to be! In fact, we'll implement
//! the counter using a loop and re-assigning:
//!
//! ```
//! fn start_counting() -> Send<Counting> {
//!     fork(|mut numbers: Recv<Counting>| async {
//!         let mut total = 0;
//!         loop {
//!             match numbers.recv1().await {
//!                 Counting::More(number) => {
//!                     let (n, next) = number.recv().await;
//!                     total += n;
//!                     numbers = next;
//!                 }
//!                 Counting::Done(report) => break report.send1(total),
//!             }
//!         }
//!     })
//! }
//! ```
//!
//! The counter's end-point of the session (`numbers`) is marked `mut`. In the case of `Counting::More`, after
//! receiving the `n` to add and the `next` continuation of the session, we simply re-assign `next` into `numbers`.
//! Note, that before the re-assignment, `numbers` has been moved-out-of in `numbers.recv1().await` -- no dropping
//! of a session happens.
//!
//! Here's how we can use the constructed counter to add up numbers between 1 and 5:
//!
//! ```
//! let sum = start_counting()
//!     .choose(Counting::More).send(1)
//!     .choose(Counting::More).send(2)
//!     .choose(Counting::More).send(3)
//!     .choose(Counting::More).send(4)
//!     .choose(Counting::More).send(5)
//!     .choose(Counting::Done).recv1().await;
//!
//! assert_eq!(sum, 15);
//! ```
//!
//! The pattern of processing an incoming stream of data is **ubiquitous enough to warrant standardization.** That's
//! what the [queue] module is. Check out its documentation for more detail! It provides two ends of a stream
//! processing queue -- [`Dequeue`](queue::Dequeue) and [`Enqueue`](queue::Enqueue) -- corresponding to the
//! `Recv<Counting>` and `Send<Counting>`, respectively. Instead of `Counting::More` and `Counting::Done`, we can use
//! [`Enqueue::push`](queue::Enqueue::push) and [`Enqueue::close`](queue::Enqueue::close). On the processing side,
//! [`Dequeue::pop`](queue::Dequeue::pop), [`Dequeue::for_each`](queue::Dequeue::for_each), and
//! [`Dequeue::fold`](queue::Dequeue::fold) are provided for ergonomic use.
//!
//! Without further explanation, the counter can be rewritten this way:
//!
//! ```
//! type Numbers = Dequeue<i64, Send<i64>>;
//! type Counter = Dual<Numbers>;  // Enqueue<i64, Recv<i64>>
//!
//! fn start_counting_with_queue() -> Counter {
//!     fork(|numbers: Numbers| async {
//!         let (total, report) = numbers
//!             .fold(0, |total, add| async move { total + add })
//!             .await;
//!         report.send1(total);
//!     })
//! }
//! ```
//!
//! And used elegantly:
//!
//! ```
//! let sum = start_counting_with_queue()
//!     .push(1)
//!     .push(2)
//!     .push(3)
//!     .push(4)
//!     .push(5)
//!     .close()
//!     .recv1()
//!     .await;
//!
//! assert_eq!(sum, 15);
//! ```
//!
//! # Multiple participants
//!
//! All session types described so far had two sides to them: an ATM versus a single client; a calculator, or
//! a counter versus a single user. That's rarely sufficient for reasonable applications. A server will handle
//! multiple clients, a game will have multiple players interacting in its world.
//!
//! Those familiar with literature will know that our session types here are _binary_. They fundamentally
//! have two sides of communication. The other side is always described by the dual. Research in session types
//! goes on to introduce _multi-party session types_ that explicitly model communication of more than two
//! sides. While they have their advantages, they also introduce complexity and (_disclaimer: personal impression_)
//! don't seem to be ready for practical use.
//!
//! What we are going to show here, however, is that **no dedicated multi-party session types are needed** to
//! model and implement protocols involving arbitrary numbers of participants. And that is **without any concessions
//! to previously described guarantees** like protocol adherence and deadlock freedom.
//!
//! The key lies in _how_ to **juggle multiple session end-points concurrently.**
//!
//! **To demonstrate,** we'll implement a very simple game of 3 players. For a general pattern of uniformly
//! handling a dynamic number of participants, check out the [server] module.
//!
//! The rules of the game are:
//!
//! 1. Each player independently picks one of two moves: _UP_ or _DOWN_.
//! 2. If everybody chooses the same move, it's a draw and the game repeats.
//! 3. Otherwise, one player must have chosen differently from the other two: that player wins!
//!
//! In other words, the choices on the left map to the outcomes on the right:
//!
//! - _**UP**, DOWN, DOWN_, or _**DOWN**, UP, UP_ -- **first** player wins.
//! - _DOWN, **UP**, DOWN_, or _UP, **DOWN**, UP_ -- **second** player wins.
//! - _DOWN, DOWN, **UP**_, or _UP, UP, **DOWN**_ -- **third** player wins.
//! - _UP, UP, UP_, or _DOWN, DOWN, DOWN_ -- **draw.**
//!
//! Let's model the game!
//!
//! ```
//! #[derive(Debug)]
//! enum Move {
//!     Up,
//!     Down,
//! }
//!
//! enum Outcome {
//!     Win,
//!     Loss,
//!     Draw(Round),
//! }
//!
//! type Round = Send<Move, Recv<Outcome>>;
//! type Player = Dual<Round>; // Recv<Move, Send<Outcome>>
//! ```
//!
//! The `Round` session is what the player sees. They are expected to send their move, then wait for an outcome
//! of the round. Two outcomes are trivial: `Win` and `Loss`. The third one, `Draw`, recursively enters the next
//! round, since no winner could be decided.
//!
//! The full game involves three players entering the game and playing according to the above protocol until
//! one of them wins.
//!
//! ```
//! #[derive(Debug)]
//! enum Winner {
//!     First,
//!     Second,
//!     Third,
//! }
//!
//! type Game = Send<(Player, Player, Player), Recv<Winner>>;
//! ```
//!
//! We model it as a session that takes in three (running) `Player`s, and reports the winner back. In between
//! those, an implementation of `Game` must take care of the three `Player` sessions by making them play.
//!
//! Here's what we can do:
//!
//! ```
//! fn start_playing() -> Game {
//!     use {Move::*, Outcome::*, Winner::*};
//!
//!     fork(|game: Dual<Game>| async {
//!         let ((mut player1, mut player2, mut player3), winner) = game.recv().await;
//!
//!         loop {
//!             let (move1, outcome1) = player1.recv().await;
//!             let (move2, outcome2) = player2.recv().await;
//!             let (move3, outcome3) = player3.recv().await;
//!
//!             tokio::time::sleep(Duration::from_secs(1)).await;
//!             println!("{:?} {:?} {:?}", move1, move2, move3);
//!             tokio::time::sleep(Duration::from_secs(1)).await;
//!
//!             match (move1, move2, move3) {
//!                 (Up, Down, Down) | (Down, Up, Up) => {
//!                     outcome1.send1(Win);
//!                     outcome2.send1(Loss);
//!                     outcome3.send1(Loss);
//!                     break winner.send1(First);
//!                 }
//!                 (Down, Up, Down) | (Up, Down, Up) => {
//!                     outcome1.send1(Loss);
//!                     outcome2.send1(Win);
//!                     outcome3.send1(Loss);
//!                     break winner.send1(Second);
//!                 }
//!                 (Down, Down, Up) | (Up, Up, Down) => {
//!                     outcome1.send1(Loss);
//!                     outcome2.send1(Loss);
//!                     outcome3.send1(Win);
//!                     break winner.send1(Third);
//!                 }
//!                 (Up, Up, Up) | (Down, Down, Down) => {
//!                     player1 = outcome1.choose(Draw);
//!                     player2 = outcome2.choose(Draw);
//!                     player3 = outcome3.choose(Draw);
//!                     println!("Draw...");
//!                 }
//!             }
//!         }
//!     })
//! }
//! ```
//!
//! Let's break it down!
//!
//! At first, we take in the three `Player` session.
//!
//! ```
//! let ((mut player1, mut player2, mut player3), winner) = game.recv().await;
//! ```
//!
//! **All three of them are now in scope** at the same time. This is the key to handling multiple
//! participants. Having access to all, the `Game` can **coordinate** them according to one another. In this
//! context it's only to decide the outcome and proceed with the game -- in other contexts, more complicated
//! mutual interactions can be implemented.
//!
//! The rest of the code is concerned with deciding the outcome and communicating it.
//!
//! - **In the case of a winner,** the `Player` sessions are terminated by either a `Win` or a `Loss`, and the
//! _pending_ `winner` response is _completed_.
//!
//! - **Otherwise in the case of a draw,** the `winner` channel is _left pending_, and draws are communicated to
//! the players, which makes them stay to play another round according to the `Round` protocol.
//!
//! **Let's have some fun playing!**
//!
//! ```
//! fn random_player() -> Player {
//!     fork(|mut round: Round| async move {
//!         while let Outcome::Draw(next_round) = round.send(random_move()).recv1().await {
//!             round = next_round;
//!         }
//!     })
//! }
//!
//! fn random_move() -> Move {
//!     if fastrand::bool() {
//!         Move::Up
//!     } else {
//!         Move::Down
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     for _ in 0..10 {
//!         let winner = start_playing()
//!             .send((random_player(), random_player(), random_player()))
//!             .recv1()
//!             .await;
//!         println!("{:?}!\n", winner);
//!     }
//! }
//! ```
//!
//! ```plain
//! Up Up Down
//! Third!
//!
//! Down Up Up
//! First!
//!
//! Down Down Down
//! Draw...
//! Up Down Up
//! Second!
//!
//! Down Up Down
//! Second!
//! ```

pub mod exchange;
pub mod queue;
pub mod runtimes;
pub mod server;

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
