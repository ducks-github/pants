// Copyright 2018 Pants project contributors (see CONTRIBUTORS.md).
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#![deny(warnings)]
// Enable all clippy lints except for many of the pedantic ones. It's a shame this needs to be copied and pasted across crates, but there doesn't appear to be a way to include inner attributes from a common source.
#![deny(
  clippy::all,
  clippy::default_trait_access,
  clippy::expl_impl_clone_on_copy,
  clippy::if_not_else,
  clippy::needless_continue,
  clippy::single_match_else,
  clippy::unseparated_literal_suffix,
  clippy::used_underscore_binding
)]
// It is often more clear to show that nothing is being moved.
#![allow(clippy::match_ref_pats)]
// Subjective style.
#![allow(
  clippy::len_without_is_empty,
  clippy::redundant_field_names,
  clippy::too_many_arguments
)]
// Default isn't as big a deal as people seem to think it is.
#![allow(clippy::new_without_default, clippy::new_ret_no_self)]
// Arc<Mutex> can be more clear than needing to grok Orderings:
#![allow(clippy::mutex_atomic)]

use std::collections::VecDeque;
use std::sync::Arc;

use futures::future::Future;
use futures::task::{self, Task};
use futures::{Async, Poll};
use parking_lot::Mutex;

struct Inner {
  waiters: VecDeque<Task>,
  available_permits: usize,
}

#[derive(Clone)]
pub struct AsyncSemaphore {
  inner: Arc<Mutex<Inner>>,
}

impl AsyncSemaphore {
  pub fn new(permits: usize) -> AsyncSemaphore {
    AsyncSemaphore {
      inner: Arc::new(Mutex::new(Inner {
        waiters: VecDeque::new(),
        available_permits: permits,
      })),
    }
  }

  ///
  /// Runs the given Future-creating function (and the Future it returns) under the semaphore.
  ///
  pub fn with_acquired<F, B, T, E>(&self, f: F) -> Box<dyn Future<Item = T, Error = E> + Send>
  where
    F: FnOnce() -> B + Send + 'static,
    B: Future<Item = T, Error = E> + Send + 'static,
  {
    Box::new(
      self
        .acquire()
        .map_err(|()| panic!("Acquisition is infalliable."))
        .and_then(|permit| {
          f().map(move |t| {
            drop(permit);
            t
          })
        }),
    )
  }

  fn acquire(&self) -> PermitFuture {
    PermitFuture {
      inner: Some(self.inner.clone()),
    }
  }
}

pub struct Permit {
  inner: Arc<Mutex<Inner>>,
}

impl Drop for Permit {
  fn drop(&mut self) {
    let task = {
      let mut inner = self.inner.lock();
      inner.available_permits += 1;
      if let Some(task) = inner.waiters.pop_front() {
        task
      } else {
        return;
      }
    };
    task.notify();
  }
}

pub struct PermitFuture {
  inner: Option<Arc<Mutex<Inner>>>,
}

impl Future for PermitFuture {
  type Item = Permit;
  type Error = ();

  fn poll(&mut self) -> Poll<Permit, ()> {
    let inner = self.inner.take().expect("cannot poll PermitFuture twice");
    let acquired = {
      let mut inner = inner.lock();
      if inner.available_permits == 0 {
        inner.waiters.push_back(task::current());
        false
      } else {
        inner.available_permits -= 1;
        true
      }
    };
    if acquired {
      Ok(Async::Ready(Permit { inner }))
    } else {
      self.inner = Some(inner);
      Ok(Async::NotReady)
    }
  }
}

#[cfg(test)]
mod tests {

  use super::AsyncSemaphore;
  use futures::{future, Future};
  use std::sync::mpsc;
  use std::thread;
  use std::time::{Duration, Instant};

  use tokio_timer::Delay;

  #[test]
  fn acquire_and_release() {
    let sema = AsyncSemaphore::new(1);

    sema
      .with_acquired(|| future::ok::<_, ()>(()))
      .wait()
      .unwrap();
  }

  #[test]
  fn at_most_n_acquisitions() {
    let sema = AsyncSemaphore::new(1);
    let handle1 = sema.clone();
    let handle2 = sema.clone();

    let (tx_thread1, acquired_thread1) = mpsc::channel();
    let (unblock_thread1, rx_thread1) = mpsc::channel();
    let (tx_thread2, acquired_thread2) = mpsc::channel();

    thread::spawn(move || {
      handle1
        .with_acquired(move || {
          // Indicate that we've acquired, and then wait to be signaled to exit.
          tx_thread1.send(()).unwrap();
          rx_thread1.recv().unwrap();
          future::ok::<_, ()>(())
        })
        .wait()
        .unwrap();
    });

    // Wait for thread1 to acquire, and then launch thread2.
    acquired_thread1
      .recv_timeout(Duration::from_secs(5))
      .expect("thread1 didn't acquire.");

    thread::spawn(move || {
      handle2
        .with_acquired(move || {
          tx_thread2.send(()).unwrap();
          future::ok::<_, ()>(())
        })
        .wait()
        .unwrap();
    });

    // thread2 should not signal until we unblock thread1.
    match acquired_thread2.recv_timeout(Duration::from_millis(100)) {
      Err(_) => (),
      Ok(_) => panic!("thread2 should not have acquired while thread1 was holding."),
    }

    // Unblock thread1 and confirm that thread2 acquires.
    unblock_thread1.send(()).unwrap();
    acquired_thread2
      .recv_timeout(Duration::from_secs(5))
      .expect("thread2 didn't acquire.");
  }

  #[test]
  fn drop_while_waiting() {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    let sema = AsyncSemaphore::new(1);
    let handle1 = sema.clone();
    let handle2 = sema.clone();

    let (tx_thread1, acquired_thread1) = mpsc::channel();
    let (unblock_thread1, rx_thread1) = mpsc::channel();
    let (tx_thread2_attempt_1, did_not_acquire_thread2_attempt_1) = mpsc::channel();
    let (tx_thread2_attempt_2, acquired_thread2_attempt_2) = mpsc::channel();

    runtime.spawn(handle1.with_acquired(move || {
      // Indicate that we've acquired, and then wait to be signaled to exit.
      tx_thread1.send(()).unwrap();
      rx_thread1.recv().unwrap();
      future::ok::<_, ()>(())
    }));

    // Wait for thread1 to acquire, and then launch thread2.
    acquired_thread1
      .recv_timeout(Duration::from_secs(5))
      .expect("thread1 didn't acquire.");

    // thread2 will wait for a little while, but then drop its PermitFuture to give up on waiting.
    runtime.spawn(future::lazy(move || {
      let permit_future = handle2.acquire();
      let delay_future = Delay::new(Instant::now() + Duration::from_millis(10));
      permit_future
        .select2(delay_future)
        .map(move |raced_result| {
          // We expect to have timed out, because the other Future will not resolve until asked.
          match raced_result {
            future::Either::A(_) => panic!("Expected to time out."),
            future::Either::B(_) => {}
          };
          tx_thread2_attempt_1.send(()).unwrap();
        })
        .map_err(|_| panic!("Permit or duration failed."))
        .and_then(move |()| {
          // Attempt again to acquire.
          handle2.acquire()
        })
        .map(move |_permit| {
          // Confirm that we did.
          tx_thread2_attempt_2.send(()).unwrap()
        })
    }));

    // thread2 should signal that it did not successfully acquire for the first attempt.
    did_not_acquire_thread2_attempt_1
      .recv_timeout(Duration::from_secs(5))
      .expect("thread2 should have failed to acquire by now.");

    // Unblock thread1 and confirm that thread2 acquires.
    unblock_thread1.send(()).unwrap();
    acquired_thread2_attempt_2
      .recv_timeout(Duration::from_secs(5))
      .expect("thread2 didn't acquire.");
  }
}
