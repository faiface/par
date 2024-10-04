//! Asynchronous forking functions for different `async` runtimes.

#[cfg(feature = "runtime-tokio")]
pub mod tokio {
    use crate::Session;
    use futures::Future;

    pub fn fork<S: Session, F>(f: impl FnOnce(S::Dual) -> F) -> S
    where
        F: Future<Output = ()> + Send + 'static,
    {
        S::fork_sync(|session| drop(tokio::spawn(f(session))))
    }
}

pub mod spawn {
    use crate::Session;
    use futures::{task::SpawnExt, Future};

    pub trait Fork {
        fn fork<S: Session, F>(&self, f: impl FnOnce(S::Dual) -> F) -> S
        where
            F: Future<Output = ()> + Send + 'static;
    }

    impl<Spawn: futures::task::Spawn> Fork for Spawn {
        fn fork<S: Session, F>(&self, f: impl FnOnce(S::Dual) -> F) -> S
        where
            F: Future<Output = ()> + Send + 'static,
        {
            S::fork_sync(|session| self.spawn(f(session)).ok().expect("spawn failed"))
        }
    }
}

pub mod local_spawn {
    use crate::Session;
    use futures::{task::LocalSpawnExt, Future};

    pub trait Fork {
        fn fork<S: Session, F>(&self, f: impl FnOnce(S::Dual) -> F) -> S
        where
            F: Future<Output = ()> + Send + 'static;
    }

    impl<Spawn: futures::task::LocalSpawn> Fork for Spawn {
        fn fork<S: Session, F>(&self, f: impl FnOnce(S::Dual) -> F) -> S
        where
            F: futures::Future<Output = ()> + Send + 'static,
        {
            S::fork_sync(|session| self.spawn_local(f(session)).ok().expect("spawn failed"))
        }
    }
}
