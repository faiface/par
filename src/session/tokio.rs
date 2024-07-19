use futures::Future;

use super::{Dual, Session};

pub fn fork<S: Session, F>(f: impl FnOnce(S) -> F) -> Dual<S>
where F: Future<Output = ()> + Send + 'static
{
    Dual::<S>::fork_sync(|session| drop(tokio::spawn(f(session))))
}
