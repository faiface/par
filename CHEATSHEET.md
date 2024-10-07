# Cheatsheet

<table>
<tr>
    <th></th>
    <th>Code</th>
</tr>
<tr>
<td style="vertical-align:top"><strong>Fork concurrently</strong></td>
<td>

```rust
// code goes here
```

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Fork synchronously</strong></td>
<td>

```rust
// code goes here
```

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Duality equations</strong></td>
<td>

```rust
use par::{Session, Dual};
use par::exchange::{Recv, Send};
use par::exchange::{Dequeue, Enqueue};
```

<table>
<tr>
    <th><code>Self</code></th>
    <th><code>Dual&lt;Self&gt;</code></th></tr>
<tr>
<tr>
    <td><code>()</code></td>
    <td><code>()</code></td>
</tr>
<tr>
    <td><code>Recv&lt;T&gt;</code></td>
    <td><code>Send&lt;T&gt;</code></td>
</tr>
<tr>
    <td><code>Send&lt;T&gt;</code></td>
    <td><code>Recv&lt;T&gt;</code></td>
</tr>
<tr>
    <td><code>Recv&lt;T, S&gt;</code></td>
    <td><code>Send&lt;T, Dual&lt;S&gt;&gt;</code></td>
</tr>
<tr>
    <td><code>Send&lt;T, S&gt;</code></td>
    <td><code>Recv&lt;T, Dual&lt;S&gt;&gt;</code></td>
</tr>
<tr>
    <td><code>Dequeue&lt;T&gt;</code></td>
    <td><code>Enqueue&lt;T&gt;</code></td>
</tr>
<tr>
    <td><code>Enqueue&lt;T&gt;</code></td>
    <td><code>Dequeue&lt;T&gt;</code></td>
</tr>
<tr>
    <td><code>Dequeue&lt;T, S&gt;</code></td>
    <td><code>Enqueue&lt;T, Dual&lt;S&gt;&gt;</code></td>
</tr>
<tr>
    <td><code>Enqueue&lt;T, S&gt;</code></td>
    <td><code>Dequeue&lt;T, Dual&lt;S&gt;&gt;</code></td>
</tr>
</table>

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Exchange last value</strong></td>
<td>

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

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Exchange a value and proceed</strong></td>
<td>

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

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Branch on a standard <code>enum</code></strong></td>
<td>

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

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Branch on a custom <code>enum</code></strong></td>
<td>

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

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Link separate dual sessions</strong></td>
<td>

```rust
use par::Session;

type MySession: Session;
type YourSession = Dual<Session>;

fn wire(me: MySession, you: YourSession) {
    me.link(you);
}
```

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Recursive session</strong></td>
<td>

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

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Queue</strong></td>
<td>

```rust
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

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Juggle multiple sessions concurrently</strong></td>
<td>

```rust
// code goes here
```

</td>
</tr>
<tr>
<td style="vertical-align:top"><strong>Server</strong></td>
<td>

```rust
// code goes here
```

</td>
</tr>
</table>
