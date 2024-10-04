use futures::{
    future,
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use par::{
    exchange::{Recv, Send},
    queue::{Dequeue, Enqueue, Queue},
    runtimes::tokio::fork,
    server::{Connection, Event, Server},
    Dual, Session,
};
use std::{
    collections::{hash_map::Entry, HashMap},
    io,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    tungstenite::{self, Message},
    WebSocketStream,
};

type WebSocketWrite = SplitSink<WebSocketStream<TcpStream>, tungstenite::Message>;
type WebSocketRead = SplitStream<WebSocketStream<TcpStream>>;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Nick(String);
struct LoginRefused;

#[derive(Clone)]
enum ChatLine {
    Message { from: Nick, content: String },
    Info(String),
    Error(String),
}

type Login = Recv<Nick, Send<Result<Send<Inbox, Recv<Conn>>, LoginRefused>>>;
type Inbox = Enqueue<ChatLine>;
type Conn = Connection<Dual<Outbox>>;
type Outbox = Recv<Command>;
enum Command {
    Message(Recv<String, Send<Conn>>),
    Logout,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = "127.0.0.1:3000";
    let listener = TcpListener::bind(addr).await?;
    eprintln!("Listening on: {}", addr);
    serve(listener).await;
    Ok(())
}

async fn serve(listener: TcpListener) {
    let mut server = Server::<Login, Outbox, Nick>::start(|proxy| {
        drop(tokio::spawn(accept_users(listener).for_each1(
            move |user| {
                future::ready(proxy.clone(|proxy| {
                    drop(tokio::spawn(async {
                        let Some(login) = user.recv1().await else {
                            return;
                        };
                        proxy.connect().link(login);
                    }))
                }))
            },
        )))
    });

    type Inboxes = HashMap<Nick, Inbox>;
    fn broadcast(inboxes: &mut Inboxes, line: ChatLine) {
        let entries = inboxes
            .drain()
            .map(|(nick, inbox)| (nick, inbox.push(line.clone())))
            .collect::<Vec<_>>();
        inboxes.extend(entries);
    }

    let mut inboxes = Inboxes::new();

    while let Some((new_server, transition)) = server.poll().await {
        server = new_server;

        match transition {
            Event::Connect { session: login } => {
                let (nick, resp) = login.recv().await;
                let Entry::Vacant(entry) = inboxes.entry(nick.clone()) else {
                    resp.send1(Err(LoginRefused));
                    continue;
                };
                let (inbox, conn) = resp.choose(Ok).recv().await;
                entry.insert(inbox);
                broadcast(&mut inboxes, ChatLine::Info(format!("{} joined", nick.0)));
                server.suspend(nick, |c| conn.send1(c));
            }

            Event::Resume {
                session: outbox,
                data: nick,
            } => match outbox.recv1().await {
                Command::Message(msg) => {
                    let (content, conn) = msg.recv().await;
                    broadcast(
                        &mut inboxes,
                        ChatLine::Message {
                            from: nick.clone(),
                            content,
                        },
                    );
                    server.suspend(nick, |c| conn.send1(c));
                }
                Command::Logout => {
                    if let Some(inbox) = inboxes.remove(&nick) {
                        inbox.close1();
                    }
                    broadcast(&mut inboxes, ChatLine::Info(format!("{} left", nick.0)));
                }
            },
        }
    }
}

fn accept_users(listener: TcpListener) -> Dequeue<Recv<Option<Login>>> {
    fork(|mut queue: Enqueue<Recv<Option<Login>>>| async move {
        while let Ok((stream, _)) = listener.accept().await {
            eprintln!("Client connecting...");

            let Ok(addr) = stream.peer_addr() else {
                eprintln!("ERROR: No peer address");
                continue;
            };
            let Ok(web_socket) = tokio_tungstenite::accept_async(stream).await else {
                eprintln!("ERROR: Handshake failed with {}", addr);
                continue;
            };

            eprintln!("New WebSocket connection: {}", addr);
            queue = queue.push(handle_user(web_socket));
        }
        queue.close1();
    })
}

fn handle_user(socket: WebSocketStream<TcpStream>) -> Recv<Option<Login>> {
    let (write, read) = socket.split();
    let inbox = handle_inbox(write);
    let messages = read_socket(read);

    fork(|try_login: Send<Option<Login>>| async {
        let inbox = inbox.push(ChatLine::Info(format!("What's your name?")));
        let Queue::Item(name, messages) = messages.pop().await else {
            inbox.close1();
            return try_login.send1(None);
        };

        let Ok(accepted) = try_login.choose(Some).send(Nick(name)).recv1().await else {
            inbox
                .push(ChatLine::Error(format!("Login refused")))
                .close1();
            return messages.for_each1(|_| future::ready(())).await;
        };

        let conn = messages
            .fold1(accepted.send(inbox).recv1().await, |conn, content| async {
                conn.resume()
                    .choose(Command::Message)
                    .send(content)
                    .recv1()
                    .await
            })
            .await;

        conn.resume().send1(Command::Logout);
    })
}

fn handle_inbox(write: WebSocketWrite) -> Inbox {
    fork(|lines: Dequeue<ChatLine>| async {
        lines
            .fold1(write, |mut write, line| async {
                write
                    .send(Message::text(match line {
                        ChatLine::Message {
                            from: Nick(name),
                            content,
                        } => format!("{}> {}", name, content),
                        ChatLine::Info(content) => format!("> {}", content),
                        ChatLine::Error(content) => format!("? {}", content),
                    }))
                    .await
                    .unwrap_or_else(|err| eprintln!("ERROR: {}", err));
                write
            })
            .await
            .close()
            .await
            .unwrap_or_else(|err| eprintln!("ERROR: {}", err));
    })
}

fn read_socket(read: WebSocketRead) -> Dequeue<String> {
    fork(|queue: Enqueue<String>| async {
        read.fold(queue, |queue, msg| async {
            match msg {
                Ok(Message::Text(content)) => queue.push(content),
                _ => queue,
            }
        })
        .await
        .close1()
    })
}
