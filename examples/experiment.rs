#![cfg(feature = "examples")]
#![allow(dead_code)]

use par::exchange::{Recv, Send};
use par::queue::Dequeue;

// ATM / CLIENT

struct Amount(i64);
struct Money(i64);
struct InsufficientFunds;

enum Operation {
    CheckBalance(Send<Amount>),
    Withdraw(Recv<Amount, Send<Result<Money, InsufficientFunds>>>),
}

struct Account(String);
struct InvalidAccount;

type ATM = Send<Account, Recv<Result<Send<Operation>, InvalidAccount>>>;

use par::{Dual, Session};

type Client = Dual<ATM>; // Recv<Account, Send<Result<Send<Operation>, InvalidAccount>>>

use par::runtimes::tokio::fork;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
//use std::time::Duration;

fn boot_atm(accounts: Arc<HashMap<String, Money>>) -> ATM {
    fork(|client: Client| async move {
        let (Account(number), client) = client.recv().await;
        let Some(&Money(funds)) = accounts.get(&number) else {
            return client.send1(Err(InvalidAccount));
        };
        match client.choose(Ok).recv1().await {
            Operation::CheckBalance(client) => client.send1(Amount(funds)),
            Operation::Withdraw(client) => {
                let (Amount(requested), client) = client.recv().await;
                if funds >= requested {
                    client.send1(Ok(Money(requested)));
                } else {
                    client.send1(Err(InsufficientFunds));
                }
            }
        }
    })
}

fn check_balance(number: String) -> Client {
    fork(|atm: ATM| async move {
        let atm = atm.send(Account(number.clone()));
        let Ok(atm) = atm.recv1().await else {
            return println!("Invalid account: {}", number);
        };
        let Amount(funds) = atm.choose(Operation::CheckBalance).recv1().await;
        println!("{} has {}", number, funds);
    })
}

fn withdraw(number: String, Amount(requested): Amount) -> Client {
    fork(|atm: ATM| async move {
        let Ok(atm) = atm.send(Account(number.clone())).recv1().await else {
            return println!("Invalid account: {}", number);
        };
        let response = atm
            .choose(Operation::Withdraw)
            .send(Amount(requested))
            .recv1()
            .await;
        match response {
            Ok(Money(withdrawn)) => println!("{} withdrawn from {}", withdrawn, number),
            Err(InsufficientFunds) => println!(
                "{} has insufficient funds to withdraw {}",
                number, requested
            ),
        }
    })
}

// COUNTING

enum Count {
    More(Recv<i64, Counting>),
    Over(Send<i64>),
}
type Counting = Recv<Count>;
type Counter = Dual<Counting>;

fn start_counting() -> Counter {
    let mut total = 0;
    fork(|mut count: Counting| async move {
        loop {
            match count.recv1().await {
                Count::More(more) => {
                    let (add, next_count) = more.recv().await;
                    total += add;
                    count = next_count;
                }
                Count::Over(over) => break over.send1(total),
            }
        }
    })
}

// QUEUE COUNTING

type QueueCounting = Dequeue<i64, Send<i64>>;
type QueueCounter = Dual<QueueCounting>;

fn start_counting_with_queue() -> QueueCounter {
    fork(|numbers: QueueCounting| async {
        let (total, over) = numbers
            .fold(0, |total, add| async move { total + add })
            .await;
        over.send1(total);
    })
}

// UP/DOWN GAME

#[derive(Debug)]
enum Move {
    Up,
    Down,
}
type Game = Send<Move, Recv<Outcome>>;
enum Outcome {
    Win,
    Loss,
    Draw(Game),
}

type Player = Dual<Game>;

#[derive(Debug)]
enum Winner {
    First,
    Second,
    Third,
}
type Judge = Send<(Player, Player, Player), Recv<Winner>>;
type Players = Dual<Judge>;

fn start_playing() -> Judge {
    use Move::*;
    use Outcome::*;
    use Winner::*;

    fork(|players: Players| async {
        let ((mut player1, mut player2, mut player3), winner) = players.recv().await;

        loop {
            let (move1, outcome1) = player1.recv().await;
            let (move2, outcome2) = player2.recv().await;
            let (move3, outcome3) = player3.recv().await;

            match (move1, move2, move3) {
                (Up, Down, Down) | (Down, Up, Up) => {
                    outcome1.send1(Win);
                    outcome2.send1(Loss);
                    outcome3.send1(Loss);
                    break winner.send1(First);
                }
                (Down, Up, Down) | (Up, Down, Up) => {
                    outcome1.send1(Loss);
                    outcome2.send1(Win);
                    outcome3.send1(Loss);
                    break winner.send1(Second);
                }
                (Down, Down, Up) | (Up, Up, Down) => {
                    outcome1.send1(Loss);
                    outcome2.send1(Loss);
                    outcome3.send1(Win);
                    break winner.send1(Third);
                }
                (Up, Up, Up) | (Down, Down, Down) => {
                    player1 = outcome1.choose(Draw);
                    player2 = outcome2.choose(Draw);
                    player3 = outcome3.choose(Draw);
                }
            }
        }
    })
}

fn random_player(name: &'static str) -> Player {
    fork(|mut game: Game| async move {
        loop {
            let my_move = if fastrand::bool() {
                Move::Up
            } else {
                Move::Down
            };
            println!("{} is playing {:?}", name, my_move);
            tokio::time::sleep(Duration::from_secs(2)).await;
            match game.send(my_move).recv1().await {
                Outcome::Win => break println!("{} won!", name),
                Outcome::Loss => break,
                Outcome::Draw(next) => game = next,
            }
        }
    })
}

#[tokio::main]
async fn main() {
    let accounts = Arc::new(HashMap::from([
        ("Alice".to_string(), Money(1000)),
        ("Bob".to_string(), Money(700)),
        ("Cyril".to_string(), Money(5500)),
    ]));

    /*let atm = boot_atm(Arc::clone(&accounts));
    let client = withdraw("Cyril".to_string(), Amount(2500));

    atm.link(client);*/

    boot_atm(Arc::clone(&accounts)).link(check_balance("Alice".to_string()));
    boot_atm(Arc::clone(&accounts)).link(withdraw("Bob".to_string(), Amount(1000)));
    boot_atm(Arc::clone(&accounts)).link(withdraw("Dylan".to_string(), Amount(20)));

    tokio::time::sleep(Duration::from_secs(1)).await;

    /*let sum = start_counting()
        .choose(Count::More).send(1)
        .choose(Count::More).send(2)
        .choose(Count::More).send(3)
        .choose(Count::More).send(4)
        .choose(Count::More).send(5)
        .choose(Count::Over).recv1().await;

    println!("{}", sum);

    let sum = start_counting_with_queue()
        .push(1)
        .push(2)
        .push(3)
        .push(4)
        .push(5)
        .close()
        .recv1()
        .await;

    println!("{}", sum);*/

    /*for _ in 0..50 {
        let alice = random_player("Alice");
        let bob = random_player("Bob");
        let cyril = random_player("Cyril");
        start_playing().send((alice, bob, cyril)).recv1().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!();
    }*/
}
