use futures::{
    future,
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use par::{
    exchange::{Recv, Send},
    queue::{Dequeue, Enqueue, Queue},
    runtimes::tokio::fork,
    server::{Connection, Server, Transition},
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
    let mut pool = Server::<Login, Outbox, Nick>::new();

    pool.proxy(|p| {
        drop(tokio::spawn(accept_users(listener).for_each1(
            move |user| {
                future::ready({
                    p.clone(|proxy| {
                        drop(tokio::spawn(async {
                            let Some(login) = user.recv1().await else {
                                return;
                            };
                            proxy.connect().link(login);
                        }))
                    })
                })
            },
        )))
    });

    type Inboxes = HashMap<Nick, Option<Inbox>>;
    fn broadcast(inboxes: &mut Inboxes, line: ChatLine) {
        for (_, option) in inboxes {
            *option = option.take().map(|inbox| inbox.push(line.clone()));
        }
    }

    let mut inboxes = Inboxes::new();
    while let Some((new_pool, transition)) = pool.poll().await {
        pool = new_pool;
        match transition {
            Transition::Connect { session: login } => {
                let (nick, resp) = login.recv().await;
                let Entry::Vacant(entry) = inboxes.entry(nick.clone()) else {
                    resp.send1(Err(LoginRefused));
                    continue;
                };
                let (inbox, conn) = resp.choose(Ok).recv().await;
                entry.insert(Some(inbox));
                broadcast(&mut inboxes, ChatLine::Info(format!("{} joined", nick.0)));
                pool.connection_with_data(nick, |c| conn.send1(c));
            }
            Transition::Resume {
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
                    pool.resume(nick, |c| conn.send1(c));
                }
                Command::Logout => {
                    if let Some(inbox) = inboxes.remove(&nick).flatten() {
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
        let Queue::Pop(name, messages) = messages.pop().await else {
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
        let _ = lines
            .fold1(write, |mut write, line| async {
                let _ = write
                    .send(Message::text(match line {
                        ChatLine::Message {
                            from: Nick(name),
                            content,
                        } => format!("{}> {}", name, content),
                        ChatLine::Info(content) => format!("> {}", content),
                        ChatLine::Error(content) => format!("? {}", content),
                    }))
                    .await;
                write
            })
            .await
            .close()
            .await;
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
