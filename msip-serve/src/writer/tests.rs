use std::io::Read;
use std::sync::mpsc;

use serde_json::json;

use msip::frame::read_msg;

use super::*;

fn pair(timeout: Duration) -> (Writer, UnixStream) {
    let (server, peer) = UnixStream::pair().unwrap();
    let size: libc::c_int = 4096;
    // SAFETY: size is an initialized integer with the exact size supplied to setsockopt.
    assert_eq!(
        unsafe {
            libc::setsockopt(
                server.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                (&size as *const libc::c_int).cast(),
                std::mem::size_of_val(&size) as libc::socklen_t,
            )
        },
        0
    );
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    (
        Writer::with_limits(server, MAX_PENDING_BYTES, timeout).unwrap(),
        peer,
    )
}

#[test]
fn concurrent_replies_and_broadcasts_remain_complete_frames() {
    let (writer, mut peer) = pair(Duration::from_secs(3));
    let threads: Vec<_> = (0..2).map(|producer| {
        let outgoing = writer.outgoing.clone();
        thread::spawn(move || {
            for sequence in 0..100 {
                outgoing.send(MsgType::Update, &json!({"producer":producer,"sequence":sequence,"body":"x".repeat(1024)})).unwrap();
            }
        })
    }).collect();
    let mut seen = [0u64; 2];
    for _ in 0..200 {
        let (kind, body) = read_msg(&mut peer).unwrap();
        assert_eq!(kind, MsgType::Update);
        let producer = body["producer"].as_u64().unwrap() as usize;
        assert_eq!(body["sequence"].as_u64().unwrap(), seen[producer]);
        seen[producer] += 1;
    }
    for producer in threads {
        producer.join().unwrap();
    }
    assert_eq!(seen, [100, 100]);
    assert!(
        writer
            .outgoing
            .flush_until(Instant::now() + Duration::from_secs(1))
    );
}

#[test]
fn pending_limit_includes_the_frame_currently_being_written() {
    let (socket, _peer) = UnixStream::pair().unwrap();
    let body = json!({"body":"x".repeat(900_000)});
    let mut encoded = Vec::new();
    write_msg(&mut encoded, MsgType::Update, &body).unwrap();
    let writer = Writer::with_limits(socket, encoded.len(), Duration::from_secs(3)).unwrap();
    writer.outgoing.send(MsgType::Update, &body).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let queue = writer.outgoing.0.queue.lock().unwrap();
        if queue.frames.is_empty() {
            assert_eq!(queue.pending_bytes, encoded.len());
            break;
        }
        drop(queue);
        assert!(
            Instant::now() < deadline,
            "writer never took its first frame"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(writer.outgoing.send(MsgType::Error, &json!({})).is_err());
    assert!(writer.outgoing.0.queue.lock().unwrap().closed);
}

#[test]
fn a_nonreading_peer_is_disconnected_without_blocking_other_output() {
    let (slow, _peer) = pair(Duration::from_millis(60));
    slow.outgoing
        .send(MsgType::Update, &json!({"body":"x".repeat(900_000)}))
        .unwrap();
    let (healthy, mut peer) = pair(Duration::from_secs(1));
    healthy
        .outgoing
        .send(MsgType::Welcome, &json!({"daemon":"healthy"}))
        .unwrap();
    assert_eq!(read_msg(&mut peer).unwrap().1["daemon"], "healthy");
    let (done, received) = mpsc::channel();
    let output = slow.outgoing.clone();
    thread::spawn(move || {
        done.send(output.flush_until(Instant::now() + Duration::from_secs(2)))
            .unwrap()
    });
    // The write timeout, not the longer explicit drain deadline, must close this peer.
    assert!(!received.recv_timeout(Duration::from_secs(1)).unwrap());
}

#[test]
fn dropping_the_connection_owner_wakes_its_blocked_reader() {
    let (writer, mut peer) = pair(Duration::from_secs(1));
    let stale_producer = writer.outgoing.clone();
    drop(writer);
    assert_eq!(peer.read(&mut [0u8; 1]).unwrap(), 0);
    assert!(stale_producer.send(MsgType::Update, &json!({})).is_err());
}

#[test]
fn a_total_drain_deadline_also_disconnects_a_slow_peer() {
    let (writer, _peer) = pair(Duration::from_secs(5));
    writer
        .outgoing
        .send(MsgType::Update, &json!({"body":"x".repeat(900_000)}))
        .unwrap();
    assert!(
        !writer
            .outgoing
            .flush_until(Instant::now() + Duration::from_millis(30))
    );
    assert!(writer.outgoing.0.queue.lock().unwrap().closed);
}
