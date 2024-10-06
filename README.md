# ⅋ - session types for Rust

[![Crates.io][crates-badge]][crates-url]
[![Docs.rs][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/par
[crates-url]: https://crates.io/crates/par
[docs-badge]: https://img.shields.io/docsrs/par
[docs-url]: https://docs.rs/par/latest/par
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/faiface/par/blob/main/LICENSE

**What's a session type, anyway?** It's a description an entire external behavior
of a concurrent, message-passing process. From the first message, through every
possible path and interaction that can be made with it, to all the ways it can
finish.

When implementing a concurrent program according to a session type, the type tells
what can happen at any point in the program. When we have to send a message, when to
select a path to continue, when to wait for someone else to make a choice and adapt.

Additionally, the types are designed to provide some useful guarantees:

- **Protocol adherence** -- Expectations delivered, obligations fulfilled.
- **Deadlock freedom** -- Cyclic communication is statically ruled out.

_Protocol adherence_ means that when interacting with a process described by a session
type, we can be sure (unless it crashes) that it will behave according to the protocol
specified by its type. There will no unexpected messages, nor forgotten obligations.
Just like we can rely on a function to return a string if it says so, we can rely on
a process to send a string if its specified anywhere within its session type.

_Deadlock freedom_ means that deadlocks can't happen, without dirty tricks anyway. It is
achieved by imposing a certain structure on how processes are connected. It can take
a bit of getting used to, but it becomes very important when implementing very complex
concurrent systems.

> ❓ Reading this, one may easily think, _"I don't see deadlocks happen in practice..."_,
and that's a valid objection! But it arises from our concurrent systems not being very
complex due to a lack of tools and types to design and implement them reliably.
At high levels of complexity, deadlocks become an issue, and having them ruled out
proves crucial.

Using session types, complex concurrent systems can be modelled and implemented with confidence,
as any type-checked program is guaranteed to adhere to its protocol, and avoid any deadlocks.
Message passing itself ensures natural synchronization.

Lastly, session types give names to concurrent concepts and patterns, which
enables high levels of abstraction and composability. That makes it easier to
reason and talk about large concurrent systems.

> 📚 The particular flavor of session types presented here is a full implementation
of propositional linear logic. However, no knowledge of linear logic is required to use
or understand this library.
