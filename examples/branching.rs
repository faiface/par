use par::{
    exchange::{Recv, Send},
    runtimes::tokio::fork,
    Dual, Session,
};
use std::{collections::HashMap, sync::Arc, time::Duration};

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
type Client = Dual<ATM>; // Recv<Account, Send<Result<Send<Operation>, InvalidAccount>>>

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

#[tokio::main]
async fn main() {
    let accounts = Arc::new(HashMap::from([
        ("Alice".to_string(), Money(1000)),
        ("Bob".to_string(), Money(700)),
        ("Cyril".to_string(), Money(5500)),
    ]));

    let atm = boot_atm(Arc::clone(&accounts));
    let client = withdraw("Cyril".to_string(), Amount(2500));

    atm.link(client);

    boot_atm(Arc::clone(&accounts)).link(check_balance("Alice".to_string()));
    boot_atm(Arc::clone(&accounts)).link(withdraw("Bob".to_string(), Amount(1000)));
    boot_atm(Arc::clone(&accounts)).link(withdraw("Dylan".to_string(), Amount(20)));

    tokio::time::sleep(Duration::from_secs(1)).await;
}
