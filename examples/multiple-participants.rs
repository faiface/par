use par::{
    exchange::{Recv, Send},
    runtimes::tokio::fork,
    Dual,
};
use std::time::Duration;

#[derive(Debug)]
enum Move {
    Up,
    Down,
}

enum Outcome {
    Win,
    Loss,
    Draw(Game),
}

type Game = Send<Move, Recv<Outcome>>;
type Player = Dual<Game>; // Recv<Move, Send<Outcome>>

#[derive(Debug)]
enum Winner {
    First,
    Second,
    Third,
}

type Judge = Send<(Player, Player, Player), Recv<Winner>>;
type Players = Dual<Judge>; // Recv<(Player, Player, Player), Send<Winner>>

fn start_playing() -> Judge {
    use {Move::*, Outcome::*, Winner::*};

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
            tokio::time::sleep(Duration::from_secs(1)).await;

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
    for _ in 0..10 {
        let alice = random_player("Alice");
        let bob = random_player("Bob");
        let cyril = random_player("Cyril");

        start_playing().send((alice, bob, cyril)).recv1().await;
        tokio::time::sleep(Duration::from_secs(1)).await;

        println!();
    }
}
