use super::Session;
use futures::{channel::mpsc, StreamExt};
use std::collections::HashMap;

pub struct Server<Connect, Resume, ConnectionData = ()>
where
    Connect: Session,
    Resume: Session,
{
    sender: Sender<Connect, Resume, usize>,
    receiver: Receiver<Connect, Resume, usize>,
    data: HashMap<usize, ConnectionData>,
    next_id: usize,
}

impl<C: Session, R: Session, D> Default for Server<C, R, D> {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub enum Transition<Connect, Resume, ConnectionData = ()>
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

pub struct Proxy<Connect: Session> {
    connect: Box<dyn SenderFn<Connect::Dual>>,
}

#[must_use]
pub struct Connection<Resume: Session> {
    resume: Box<dyn FnOnce(Resume::Dual) + Send>,
}

struct Sender<C: Session, R: Session, D>(mpsc::Sender<Exchange<C, R, D>>);
struct Receiver<C: Session, R: Session, D>(mpsc::Receiver<Exchange<C, R, D>>);
type Exchange<C, R, D> = (Sender<C, R, D>, Transition<C, R, D>);

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

impl<C: Session, R: Session, D> Server<C, R, D> {
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

    pub fn connection_with_data(&mut self, data: D, f: impl FnOnce(Connection<R::Dual>)) {
        let sender = self.sender.clone();
        let id = self.next_id;
        self.next_id += 1;
        self.data.insert(id, data);
        f(Connection {
            resume: Box::new(move |session| {
                sender
                    .0
                    .clone()
                    .try_send((sender, Transition::Resume { session, data: id }))
                    .expect("pool dropped");
            }),
        })
    }

    pub fn connection(&mut self, f: impl FnOnce(Connection<R::Dual>))
    where
        D: Default,
    {
        self.connection_with_data(D::default(), f)
    }

    pub fn resume(&mut self, data: D, f: impl FnOnce(Connection<R::Dual>)) {
        self.connection_with_data(data, f)
    }

    #[must_use]
    pub async fn poll(mut self) -> Option<(Self, Transition<C, R, D>)> {
        drop(self.sender);
        match self.receiver.0.next().await {
            Some((sender, trans)) => {
                self.sender = sender;
                let trans = match trans {
                    Transition::Connect { session } => Transition::Connect { session },
                    Transition::Resume { session, data: id } => {
                        let data = self.data.remove(&id).expect("missing connection data");
                        Transition::Resume { session, data }
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
