//! Handle a dynamic number of clients with connection and interaction protocols
//! specified by session types. Start a [`Server`], initiate the connection protocol
//! via a [`Proxy`], and obtain an active [`Connection`] to resume the interaction
//! later.
//!
//! The communication structure of a server is composed of three parts:
//! 
//! - [`Server`] is the central manager of the clients. Use [`poll`](Server::poll) to handle
//!   new connections (coming from [proxies](Proxy)), as well as resumptions of existing ones.
//!   Use [`suspend`](Server::suspend) to create [`Connection`] handles for later resumption.
//! - [`Proxy`] is a handle for initiating new connections. It can be dropped, or duplicated
//!   using (atypical) [`clone`](Proxy::clone). Use [`connect`](Proxy::connect) to initiate
//!   a connection protocol.
//! - [`Connection`] is an active, suspended connection to the server. It must not
//!   be dropped, disconnecting from a server should only be done in a controlled way specified
//!   in its interaction (resumption) protocol. Use [`resume`](Connection::resume) to enter the
//!   server's event loop.
//!
//! To maintain the guarantees of protocol adherance and deadlock freedom, **none two of those
//! parts can ever be in the same scope,** see each other. That is including two parts of the same
//! type! (One [`Proxy`] cannot see another [`Proxy`].)
//!
//! The signatures of the methods maintain the scoping. For example, [`Proxy::clone`] does not
//! return a new [`Proxy`], but passes it to a closure.
//!
//! ```
//! let proxy1: Proxy<Connect>;
//! proxy1.clone(|proxy2| {
//!     // I see proxy2, but not proxy1
//! })
//! // I see proxy1, but not proxy2
//! ```
//!
//! **Note:** If the scoping was not maintained, deadlocks could occur. For example, having a [`Server`] and
//! a [`Connection`] in the same scope, one could call [`resume`](Connection::resume) and get stuck
//! on the resumption because the server would not have a chance to call [`poll`](Server::poll).
//!
//! ## Usage
//!
//! TODO
//!
//! ## Correspondence to linear logic
//!
//! [Proxies](Proxy) are an implementation of coexponentials. While not a standard part of linear logic,
//! they come from a nice paper from 2021,
//! ['Client-server sessions in linear logic.'](https://dl.acm.org/doi/10.1145/3473567)

use super::Session;
use futures::{channel::mpsc, StreamExt};
use std::collections::HashMap;

#[must_use]
pub struct Server<Connect, Resume, ConnectionData>
where
    Connect: Session,
    Resume: Session,
{
    sender: Sender<Connect, Resume, usize>,
    receiver: Receiver<Connect, Resume, usize>,
    data: HashMap<usize, ConnectionData>,
    next_id: usize,
}

pub struct Proxy<Connect: Session> {
    connect: Box<dyn SenderFn<Connect::Dual>>,
}

#[must_use]
pub struct Connection<Resume: Session> {
    resume: Box<dyn FnOnce(Resume::Dual) + Send>,
}

#[must_use]
pub enum Event<Connect, Resume, ConnectionData = ()>
where
    Connect: Session,
    Resume: Session,
{
    Connect {
        session: Connect,
    },
    Resume {
        session: Resume,
        data: ConnectionData,
    },
}

impl<C: Session, R: Session, D> Server<C, R, D> {
    /// Creates a new [`Server`] and passes a [`Proxy`] to it to the provided closure. Use the proxy
    /// to initiate connections to the server and the returned server to [poll](Self::poll)
    /// [events](Event) of initiating and resuming connections.
    #[must_use]
    pub fn start(f: impl FnOnce(Proxy<C::Dual>)) -> Self {
        let (tx, rx) = mpsc::channel(0);

        let mut proxy_sender = Sender(tx.clone());
        f(Proxy {
            connect: Box::new(move |session| {
                proxy_sender
                    .0
                    .try_send((proxy_sender.clone(), Event::Connect { session }))
                    .expect("server dropped");
            }),
        });

        Self {
            sender: Sender(tx),
            receiver: Receiver(rx),
            data: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn suspend(&mut self, data: D, f: impl FnOnce(Connection<R::Dual>)) {
        let sender = self.sender.clone();
        let id = self.next_id;
        self.next_id += 1;
        self.data.insert(id, data);
        f(Connection {
            resume: Box::new(move |session| {
                sender
                    .0
                    .clone()
                    .try_send((sender, Event::Resume { session, data: id }))
                    .expect("server dropped");
            }),
        })
    }

    #[must_use]
    pub async fn poll(mut self) -> Option<(Self, Event<C, R, D>)> {
        drop(self.sender);
        match self.receiver.0.next().await {
            Some((sender, trans)) => {
                self.sender = sender;
                let trans = match trans {
                    Event::Connect { session } => Event::Connect { session },
                    Event::Resume { session, data: id } => {
                        let data = self.data.remove(&id).expect("missing connection data");
                        Event::Resume { session, data }
                    }
                };
                Some((self, trans))
            }
            None => None,
        }
    }
}

impl<C: Session> Proxy<C> {
    pub fn clone(&self, f: impl FnOnce(Self)) {
        f(Self {
            connect: self.connect.clone(),
        })
    }

    #[must_use]
    pub fn connect(self) -> C {
        C::fork_sync(|dual| self.connect.send(dual))
    }
}

impl<R: Session> Connection<R> {
    #[must_use]
    pub fn resume(self) -> R {
        R::fork_sync(|dual| (self.resume)(dual))
    }
}

struct Sender<C: Session, R: Session, D>(mpsc::Sender<Exchange<C, R, D>>);
struct Receiver<C: Session, R: Session, D>(mpsc::Receiver<Exchange<C, R, D>>);
type Exchange<C, R, D> = (Sender<C, R, D>, Event<C, R, D>);

impl<C: Session, R: Session, D> Clone for Sender<C, R, D> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

trait SenderFn<T>: Send + Sync + 'static {
    fn clone(&self) -> Box<dyn SenderFn<T>>;
    fn send(self: Box<Self>, value: T);
}

impl<T, F: FnOnce(T) + Send + Sync + Clone + 'static> SenderFn<T> for F {
    fn clone(&self) -> Box<dyn SenderFn<T>> {
        Box::new((self as &F).clone())
    }

    fn send(self: Box<Self>, value: T) {
        self(value)
    }
}
