//! One bounded, serialized output stream per connection.

use std::collections::VecDeque;
use std::io;
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use msip::frame::{FrameError, MsgType, write_msg};

const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
pub(super) const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

struct Frame {
    sequence: u64,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Queue {
    frames: VecDeque<Frame>,
    /// Includes the frame currently being written, until its final byte is sent.
    pending_bytes: usize,
    submitted: u64,
    delivered: u64,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
    socket: UnixStream,
    max_pending_bytes: usize,
    write_timeout: Duration,
}

/// Cloneable producer; only the writer thread ever sends bytes to the socket.
#[derive(Clone)]
pub(super) struct Outgoing(Arc<Shared>);

/// Owns the writer lifetime even if the connection reader unwinds.
pub(super) struct Writer {
    pub outgoing: Outgoing,
}

impl Writer {
    pub fn new(socket: UnixStream) -> io::Result<Self> {
        Self::with_limits(socket, MAX_PENDING_BYTES, DRAIN_TIMEOUT)
    }

    fn with_limits(
        socket: UnixStream,
        max_pending_bytes: usize,
        write_timeout: Duration,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            changed: Condvar::new(),
            socket,
            max_pending_bytes,
            write_timeout,
        });
        let outgoing = Outgoing(shared);
        let writing = outgoing.clone();
        thread::Builder::new()
            .name("msip-output".into())
            .spawn(move || writing.run())?;
        Ok(Self { outgoing })
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.outgoing.close();
    }
}

impl Outgoing {
    pub fn send<T: Serialize>(&self, kind: MsgType, body: &T) -> Result<(), FrameError> {
        let mut bytes = Vec::new();
        write_msg(&mut bytes, kind, body)?;
        let mut queue = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.closed {
            return Err(disconnected());
        }
        if bytes.len() > self.0.max_pending_bytes.saturating_sub(queue.pending_bytes) {
            drop(queue);
            self.close();
            return Err(disconnected());
        }
        let Some(sequence) = queue.submitted.checked_add(1) else {
            drop(queue);
            self.close();
            return Err(disconnected());
        };
        queue.submitted = sequence;
        queue.pending_bytes += bytes.len();
        queue.frames.push_back(Frame { sequence, bytes });
        self.0.changed.notify_all();
        Ok(())
    }

    /// Wait for frames already enqueued, never for later traffic. Call without the conversation lock.
    pub fn flush_until(&self, deadline: Instant) -> bool {
        let mut queue = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
        let target = queue.submitted;
        while queue.delivered < target && !queue.closed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                drop(queue);
                self.close();
                return false;
            }
            let (next, _) = self
                .0
                .changed
                .wait_timeout(queue, remaining)
                .unwrap_or_else(|e| e.into_inner());
            queue = next;
        }
        queue.delivered >= target
    }

    fn close(&self) {
        let mut queue = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.closed = true;
        queue.frames.clear();
        let _ = self.0.socket.shutdown(Shutdown::Both);
        self.0.changed.notify_all();
    }

    fn run(&self) {
        loop {
            let frame = {
                let mut queue = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
                while queue.frames.is_empty() && !queue.closed {
                    queue = self
                        .0
                        .changed
                        .wait(queue)
                        .unwrap_or_else(|e| e.into_inner());
                }
                if queue.closed {
                    return;
                }
                queue
                    .frames
                    .pop_front()
                    .expect("a queued frame is available")
            };
            if send_frame(&self.0.socket, &frame.bytes, self.0.write_timeout).is_err() {
                self.close();
                return;
            }
            let mut queue = self.0.queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.pending_bytes -= frame.bytes.len();
            queue.delivered = frame.sequence;
            self.0.changed.notify_all();
        }
    }
}

fn disconnected() -> FrameError {
    io::Error::new(io::ErrorKind::BrokenPipe, "MSIP output connection closed").into()
}

/// MSG_DONTWAIT affects this send only: the cloned descriptor used by the reader stays blocking.
fn send_frame(socket: &UnixStream, mut bytes: &[u8], timeout: Duration) -> io::Result<()> {
    let mut deadline = Instant::now() + timeout;
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        // SAFETY: bytes remains readable for its full length and socket owns the borrowed fd.
        let sent = unsafe {
            libc::send(
                socket.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
            )
        };
        if sent > 0 {
            bytes = &bytes[sent as usize..];
            deadline = Instant::now() + timeout;
            continue;
        }
        if sent == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::Interrupted => continue,
            io::ErrorKind::WouldBlock => wait_writable(socket, deadline)?,
            _ => return Err(error),
        }
    }
    Ok(())
}

fn wait_writable(socket: &UnixStream, deadline: Instant) -> io::Result<()> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut descriptor = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        let timeout_ms = remaining.as_millis().max(1).min(i32::MAX as u128) as i32;
        // SAFETY: descriptor is one writable pollfd; the socket stays owned through the call.
        let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if ready > 0 {
            return Ok(());
        }
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

#[cfg(test)]
mod tests;
