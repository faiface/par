use std::collections::HashMap;

use futures::{channel::mpsc, StreamExt};

use super::Session;

pub struct Pool<Connect, Enter, ConnectionData = ()>
where
    Connect: Session,
    Enter: Session,
{
    sender: Sender<Connect, Enter, usize>,
    receiver: Receiver<Connect, Enter, usize>,
    data: HashMap<usize, ConnectionData>,
    next_id: usize,
}

impl<C: Session, E: Session, D> Default for Pool<C, E, D> {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub enum Transition<Connect, Enter, ConnectionData = ()>
where
    Connect: Session,
    Enter: Session,
{
    Connect {
        session: Connect,
    },
    Enter {
        session: Enter,
        data: ConnectionData,
    },
}

pub struct Proxy<Connect: Session> {
    connect: Box<dyn SenderFn<Connect::Dual>>,
}

#[must_use]
pub struct Connection<Enter: Session> {
    enter: Box<dyn FnOnce(Enter::Dual) + Send>,
}

struct Sender<C: Session, E: Session, D>(mpsc::Sender<Exchange<C, E, D>>);
struct Receiver<C: Session, E: Session, D>(mpsc::Receiver<Exchange<C, E, D>>);
type Exchange<C, E, D> = (Sender<C, E, D>, Transition<C, E, D>);

impl<C: Session, E: Session, D> Clone for Sender<C, E, D> {
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

impl<C: Session, E: Session, D> Pool<C, E, D> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(0);
        Self {
            sender: Sender(tx),
            receiver: Receiver(rx),
            data: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn proxy(&self, f: impl FnOnce(Proxy<C::Dual>)) {
        let mut sender = self.sender.clone();
        f(Proxy {
            connect: Box::new(move |session| {
                sender
                    .0
                    .try_send((sender.clone(), Transition::Connect { session }))
                    .expect("pool dropped");
            }),
        })
    }

    pub fn connection_with_data(&mut self, data: D, f: impl FnOnce(Connection<E::Dual>)) {
        let sender = self.sender.clone();
        let id = self.next_id;
        self.next_id += 1;
        self.data.insert(id, data);
        f(Connection {
            enter: Box::new(move |session| {
                sender
                    .0
                    .clone()
                    .try_send((sender, Transition::Enter { session, data: id }))
                    .expect("pool dropped");
            }),
        })
    }

    pub fn connection(&mut self, f: impl FnOnce(Connection<E::Dual>))
    where
        D: Default,
    {
        self.connection_with_data(D::default(), f)
    }

    pub fn resume(&mut self, data: D, f: impl FnOnce(Connection<E::Dual>)) {
        self.connection_with_data(data, f)
    }

    #[must_use]
    pub async fn poll(mut self) -> Option<(Self, Transition<C, E, D>)> {
        drop(self.sender);
        match self.receiver.0.next().await {
            Some((sender, trans)) => {
                self.sender = sender;
                let trans = match trans {
                    Transition::Connect { session } => Transition::Connect { session },
                    Transition::Enter { session, data: id } => {
                        let data = self.data.remove(&id).expect("missing connection data");
                        Transition::Enter { session, data }
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

impl<E: Session> Connection<E> {
    #[must_use]
    pub fn enter(self) -> E {
        E::fork_sync(|dual| (self.enter)(dual))
    }
}
