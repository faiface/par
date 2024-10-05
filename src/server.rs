//! Handle a dynamic number of clients with interaction protocols specified by session types.
//! Start a [`Server`], initiate the connection protocol via a [`Proxy`], and obtain an active
//! [`Connection`] to resume the interaction later.
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
//! To maintain the guarantees of protocol adherence and deadlock freedom, **none two of those
//! parts can ever be in the same scope** (see each other). That is including two parts of the same
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
//! ## Correspondence to linear logic
//!
//! [Proxies](Proxy) are an implementation of coexponentials. While not a standard part of linear logic,
//! they come from a nice paper from 2021,
//! ['Client-server sessions in linear logic.'](https://dl.acm.org/doi/10.1145/3473567)

use super::Session;
use futures::{channel::mpsc, StreamExt};
use std::collections::HashMap;

/// Listens to connection initiatioins (from a [`Proxy`]) and resumptions (from a [`Connection`]) and
/// maintains local data for each active connection. Use [`suspend`](Self::suspend) to create or maintain
/// active an connection.
///
/// The three generic parameters are as follows:
///
/// - **`Connect`** -- The connection initiation protocol. Involves any communication needed for
///   establishing a valid connection, such as login details. A successful initiation should result in
///   the client receiving a [`Connection`] at the end of the protocol; the server can be used to create
///   it by calling [`suspend`](Self::suspend).
///
/// - **`Resume`** -- The connection resumption protocol. When a client obtains a [`Connection`], it will
///   use it to resume the interaction using this protocol. Any suspended [`Connection`] will be resumed
///   and enter the server's event loop along with its local connection data. The protocol may result
///   in the client obtaining a [`Connection`] again -- the connection is kept active -- or not, in which
///   case the connection is ended.
///
/// - **`ConnectionData`** -- Local data associated with each active connection. The data can be used to
///   identify the client, as well as carry state that's updated with each suspension. Only the server side
///   can access this data directly.
///
/// Must not be dropped.
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

/// A handle for initiating new connections with the corresponding [server](Server). Use
/// [`connect`](Self::connect) to start the connection initiation protocol.
///
/// Can be duplicated with [`Proxy::clone`], or dropped at will.
pub struct Proxy<Connect: Session> {
    connect: Box<dyn SenderFn<Connect::Dual>>,
}

/// A client's handle to an active suspended connection. Use [`resume`](Self::resume) to enter the
/// server's event loop and continue interaction.
///
/// Must not be dropped. Specify the resumption protocol to handle disconnecting.
#[must_use]
pub struct Connection<Resume: Session> {
    resume: Box<dyn FnOnce(Resume::Dual) + Send>,
}

/// Connection initiation or resumption event.
#[must_use]
pub enum Event<Connect, Resume, ConnectionData>
where
    Connect: Session,
    Resume: Session,
{
    /// Connection initiation event.
    Connect {
        /// Server's end-point of the initiation protocol.
        session: Connect,
    },

    /// Connection resumption event.
    Resume {
        /// Server's end-point of the resumption protocol.
        session: Resume,

        /// Local connection data previously assigned with [`suspend`](Server::suspend).
        data: ConnectionData,
    },
}

impl<Connect: Session, Resume: Session, ConnectionData> Server<Connect, Resume, ConnectionData> {
    /// Creates a new [`Server`] and passes a [`Proxy`] to it to the provided closure. Use the [`Proxy`]
    /// to initiate connections to the server and use the returned [`Server`]` to [poll](Self::poll)
    /// [events](Event) of initiating and resuming connections.
    #[must_use]
    pub fn start(f: impl FnOnce(Proxy<Connect::Dual>)) -> Self {
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

    /// Creates or maintains an active connection and associates local data to it. The data will later
    /// come back with a [resumption event](Event::Resume). Use this method to pass a [`Connection`] to
    /// a client during the initiation and resumption protocols.
    pub fn suspend(&mut self, data: ConnectionData, f: impl FnOnce(Connection<Resume::Dual>)) {
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

    /// Waits for the next connection initiation (from a [`Proxy`]) or resumption (from a [`Connection`]).
    /// Returns the corresponding event along with a new [`Server`] handle. In case no more [proxies](Proxy)
    /// or [connections](Connection) exist, [`None`] is returned and the [`Server`] is dropped.
    #[must_use]
    pub async fn poll(mut self) -> Option<(Self, Event<Connect, Resume, ConnectionData>)> {
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

impl<Connect: Session> Proxy<Connect> {
    /// Duplicates the [proxy](Proxy) into another scope. No two [proxies](Proxy) can ever see each
    /// other, but there can be an arbitrary number of them around.
    pub fn clone(&self, f: impl FnOnce(Self)) {
        f(Self {
            connect: self.connect.clone(),
        })
    }

    /// Initiates a new connection with the server. Returns the client's side of the connection
    /// initiation protocol.
    #[must_use]
    pub fn connect(self) -> Connect {
        Connect::fork_sync(|dual| self.connect.send(dual))
    }
}

impl<Resume: Session> Connection<Resume> {
    /// Resumes the [connection](Connection), entering the server's event loop. Returns the client's
    /// side of the connection resumption protocol.
    #[must_use]
    pub fn resume(self) -> Resume {
        Resume::fork_sync(|dual| (self.resume)(dual))
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
