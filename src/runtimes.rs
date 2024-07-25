pub mod tokio {
    use futures::Future;

    use crate::Session;

    pub fn fork<S: Session, F>(f: impl FnOnce(S::Dual) -> F) -> S
    where
        F: Future<Output = ()> + Send + 'static,
    {
        S::fork_sync(|session| drop(tokio::spawn(f(session))))
    }
}

pub mod spawn {
    use futures::{task::SpawnExt, Future};

    use crate::Session;

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
    use futures::{task::LocalSpawnExt, Future};

    use crate::Session;

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
