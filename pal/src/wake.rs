//! Wakes the event loop of the pal from other threads.
//!
//! The event loop waits on the X11 connection and on the read end of a
//! socket pair. A `Waker` writes one byte to the other end, so that the
//! event loop wakes up and reads its channels.

use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

/// The write end. Clones share the socket.
#[derive(Clone)]
pub struct Waker(Arc<UnixStream>);

/// The read end, for the event loop.
pub struct WakeSource(UnixStream);

/// A connected pair of a waker and its source.
pub fn pair() -> std::io::Result<(Waker, WakeSource)> {
    let (tx, rx) = UnixStream::pair()?;
    tx.set_nonblocking(true)?;
    rx.set_nonblocking(true)?;
    Ok((Waker(Arc::new(tx)), WakeSource(rx)))
}

impl Waker {
    /// Wakes the event loop. A full socket already holds a wake-up, so a
    /// failed write needs no retry.
    pub fn wake(&self) {
        let _ = (&*self.0).write(&[1]);
    }
}

impl WakeSource {
    /// Reads all pending wake-ups.
    pub fn drain(&mut self) {
        let mut buf = [0u8; 64];
        while matches!(self.0.read(&mut buf), Ok(n) if n > 0) {}
    }
}

impl AsFd for WakeSource {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wake_up_is_readable_once() {
        let (waker, mut source) = pair().unwrap();
        waker.wake();
        waker.clone().wake();
        let mut buf = [0u8; 8];
        assert_eq!(source.0.read(&mut buf).unwrap(), 2);
        source.drain();
        assert!(source.0.read(&mut buf).is_err());
    }
}
