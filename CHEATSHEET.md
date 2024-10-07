# Cheatsheet

## Fork concurrently

```rust
use par::{Session, Dual};

type MySession: Session;
type YourSession = Dual<MySession>;

use par::runtimes::tokio::fork;

let me: MySession = fork(|you: YourSession| async {
    // handle 'you', asynchronously
});
// handle 'me'
```

## Fork synchronously

```rust
let you: YourSession = MySession::fork_sync(|me| {
    // handle 'me'
    // this body completes before `fork_sync` returns
});
// handle 'you'
```

## Duality equations

```rust
use par::{Session, Dual};
use par::exchange::{Recv, Send};
use par::exchange::{Dequeue, Enqueue};
```

| `Self` | `Dual<Self>` |
| --- | --- |
| `()` | `()` |
| `Recv<T>` | `Send<T>` |
| `Send<T>` | `Recv<T>` |
| `Recv<T, S>` | `Send<T, Dual<S>>` |
| `Send<T, S>` | `Recv<T, Dual<S>>` |
| `Recv<Result<S1, S2>>` | `Send<Result<S1, S2>>` |
| `Send<Result<S1, S2>>` | `Recv<Result<S1, S2>>` |
| `Dequeue<T>` | `Enqueue<T>` |
| `Enqueue<T>` | `Dequeue<T>` |
| `Dequeue<T, S>` | `Enqueue<T, Dual<S>>` |
| `Enqueue<T, S>` | `Dequeue<T, Dual<S>>` |

## Exchange last value

```rust
use par::exchange::{Recv, Send};
```

```rust
// me: Send<i64>
me.send1(7);
```

```rust
// you: Recv<i64>
let value = you.recv1().await;
```

## Exchange a value and proceed

```rust
use par::{Session, Dual};

type MySide: Session;
type YourSide = Dual<MySide>;
```

```rust
// me: Recv<i64, MySide>
let (value, me) = me.recv().await;
// me: MySide
```

```rust
// you: Send<i64, YourSide>
let you = you.send(7);
// you: YourSide
```

## Branch on a standard <code>enum</code>

```rust
// me: Recv<Option<Recv<i64>>>
match me.recv().await {
    Some(me) => {
        // me: Recv<i64>
        let value = me.recv1().await;
    }
    None => (),
}
```

```rust
// you: Send<Option<Recv<i64>>>
let you = you.choose(Some);
// you: Send<i64>
you.send1(7);
```

## Branch on a custom <code>enum</code>

```rust
enum Choice {
    One(Recv<i64>),
    Two(Recv<i64, Recv<i64>>),
    Three(Recv<i64, Recv<i64, Recv<i64>>>),
}
```

```rust
// me: Send<Choice>
let me = me.choose(Choice::Two);
// me: Send<i64, Send<i64>>
me.send(7).send1(11);
```

```rust
// you: Recv<Choice>
match you.recv().await {
    /* ... */
}
```

## Link separate dual sessions

```rust
use par::Session;

type MySession: Session;
type YourSession = Dual<Session>;

fn wire(me: MySession, you: YourSession) {
    me.link(you);
}
```

## Recursive session

```rust
enum Counting {
    More(Recv<i64, Recv<Counting>>),
    Done(Send<i64>),
}

fn start_counting() -> Send<Counting> {
    fork(|mut numbers: Recv<Counting>| async {
        let mut total = 0;
        loop {
            match numbers.recv1().await {
                Counting::More(number) => {
                    let (n, next) = number.recv().await;
                    total += n;
                    numbers = next;
                }
                Counting::Done(report) => break report.send1(total),
            }
        }
    })
}
```

```rust
let sum = start_counting()
    .choose(Counting::More).send(3)
    .choose(Counting::More).send(5)
    .choose(Counting::Done).recv1().await;

assert_eq!(sum, 8);
```

## Queue

```rust
use par::queue::Dequeue;

type Numbers = Dequeue<i64, Send<i64>>;
type Counter = Dual<Numbers>;

fn start_counting_with_queue() -> Counter {
    fork(|numbers: Numbers| async {
        let (total, report) = numbers
            .fold(0, |total, add| async move { total + add })
            .await;
        report.send1(total);
    })
}
```

```rust
let sum = start_counting_with_queue()
    .push(3)
    .push(5)
    .close()
    .recv1()
    .await;

assert_eq!(sum, 8);
```

## Juggle multiple sessions concurrently

```rust
// TODO
```

## Server

```rust
// TODO
```
