use par::{
    exchange::{Recv, Send},
    runtimes::tokio::fork,
    Dual,
};

enum Op {
    Plus,
    Times,
}

type Calculator = Send<i64, Send<Op, Send<i64, Recv<i64>>>>;
type User = Dual<Calculator>; // Recv<i64, Recv<Op, Recv<i64, Send<i64>>>>

fn start_calculator() -> Calculator {
    fork(|user: User| async {
        let (x, user) = user.recv().await;
        let (op, user) = user.recv().await;
        let (y, user) = user.recv().await;
        let result = match op {
            Op::Plus => x + y,
            Op::Times => x * y,
        };
        user.send1(result);
    })
}

#[tokio::main]
async fn main() {
    let sum = start_calculator()
        .send(3)
        .send(Op::Plus)
        .send(4)
        .recv1()
        .await;

    let product = start_calculator()
        .send(3)
        .send(Op::Times)
        .send(4)
        .recv1()
        .await;

    assert_eq!(sum, 7);
    assert_eq!(product, 12);
}
