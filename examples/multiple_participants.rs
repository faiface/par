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
    Draw(Round),
}

type Round = Send<Move, Recv<Outcome>>;
type Player = Dual<Round>; // Recv<Move, Send<Outcome>>

#[derive(Debug)]
enum Winner {
    First,
    Second,
    Third,
}

type Game = Send<(Player, Player, Player), Recv<Winner>>;

fn start_playing() -> Game {
    use {Move::*, Outcome::*, Winner::*};

    fork(|game: Dual<Game>| async {
        let ((mut player1, mut player2, mut player3), winner) = game.recv().await;

        loop {
            let (move1, outcome1) = player1.recv().await;
            let (move2, outcome2) = player2.recv().await;
            let (move3, outcome3) = player3.recv().await;

            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("{:?} {:?} {:?}", move1, move2, move3);
            tokio::time::sleep(Duration::from_secs(1)).await;

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
                    println!("Draw...");
                }
            }
        }
    })
}

fn random_player() -> Player {
    fork(|mut round: Round| async move {
        while let Outcome::Draw(next_round) = round.send(random_move()).recv1().await {
            round = next_round;
        }
    })
}

fn random_move() -> Move {
    if fastrand::bool() {
        Move::Up
    } else {
        Move::Down
    }
}

#[tokio::main]
async fn main() {
    for _ in 0..10 {
        let winner = start_playing()
            .send((random_player(), random_player(), random_player()))
            .recv1()
            .await;
        println!("{:?}!\n", winner);
    }
}
