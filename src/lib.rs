//! Session types are used to model communication protocols for message-passing
//! concurrent systems and type-check their implementations, guaranteeing:
//!
//! - **Protocol adherence** -- Expectations delivered, obligations fulfilled.
//! - **Deadlock freedom** -- The two ends of each exchange are always isolated from each other.
//!
//! This not only prevents a range of concurrency-related bugs, but also helps
//! with designing concurrent protocols. An unsound protocol not covering all of its
//! inherent paths of execution will be exposed while attempting to implement it.
//!
//! Additionally, complex concurrent systems can be modelled and implemented
//! with confidence, as any type-checked implementation is guaranteed to adhere
//! to its protocol, and avoid any deadlocks.
//!
//! _The particular flavor of session types presented here is a full implementation
//! of propositional linear logic. No knowledge of linear logic is required to use
//! this library. Nevertheless, using these session types can help a great deal
//! in understanding linear logic, if one so desires._

pub mod exchange;
pub mod pool;
pub mod queue;
pub mod tokio;

pub trait Session: Send + 'static {
    type Dual: Session<Dual = Self>;

    #[must_use]
    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self;
    fn link(self, dual: Self::Dual);
}

pub type Dual<S> = <S as Session>::Dual;

impl Session for () {
    type Dual = ();
    fn fork_sync(f: impl FnOnce(Self::Dual)) -> Self {
        f(())
    }
    fn link(self, (): Self::Dual) {}
}
