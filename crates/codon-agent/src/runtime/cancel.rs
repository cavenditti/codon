//! Cooperative cancellation token. `Agent::run` polls it between model
//! calls and between tool calls, and additionally races
//! [`CancelToken::cancelled`] against the model stream so an in-flight
//! completion aborts promptly even when the provider has gone quiet.
//! Tool implementations that do long-running work should also accept
//! the token and short-circuit when it fires.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Default, Debug)]
struct CancelInner {
    flag: AtomicBool,
    wakers: Mutex<Vec<Waker>>,
}

#[derive(Clone, Default, Debug)]
pub struct CancelToken {
    inner: Arc<CancelInner>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.inner.flag.store(true, Ordering::SeqCst);
        let wakers = match self.inner.wakers.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        for waker in wakers {
            waker.wake();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.flag.load(Ordering::SeqCst)
    }

    /// Future that resolves when [`Self::cancel`] fires (immediately if
    /// it already has). Used to race cancellation against the model
    /// stream inside the agent loop.
    pub fn cancelled(&self) -> Cancelled {
        Cancelled {
            inner: self.inner.clone(),
        }
    }
}

pub struct Cancelled {
    inner: Arc<CancelInner>,
}

impl Future for Cancelled {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.inner.flag.load(Ordering::SeqCst) {
            return Poll::Ready(());
        }
        match self.inner.wakers.lock() {
            Ok(mut wakers) => wakers.push(cx.waker().clone()),
            Err(poisoned) => poisoned.into_inner().push(cx.waker().clone()),
        }
        // Re-check after registering: a `cancel()` racing between the
        // flag load above and the waker push would otherwise be missed.
        if self.inner.flag.load(Ordering::SeqCst) {
            return Poll::Ready(());
        }
        Poll::Pending
    }
}
