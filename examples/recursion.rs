use par::{
    exchange::{Recv, Send},
    queue::Dequeue,
    runtimes::tokio::fork,
    Dual,
};

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

#[tokio::main]
async fn main() {
    let sum = start_counting()
        .choose(Counting::More)
        .send(1)
        .choose(Counting::More)
        .send(2)
        .choose(Counting::More)
        .send(3)
        .choose(Counting::More)
        .send(4)
        .choose(Counting::More)
        .send(5)
        .choose(Counting::Done)
        .recv1()
        .await;

    assert_eq!(sum, 15);

    let sum = start_counting_with_queue()
        .push(1)
        .push(2)
        .push(3)
        .push(4)
        .push(5)
        .close()
        .recv1()
        .await;

    assert_eq!(sum, 15);
}
