use super::*;

#[tokio::test]
async fn limits_count_utf8_bytes_and_release_every_reservation() {
    let admission = Admission::default();
    assert!(
        admission
            .admit("x".repeat(MAX_MESSAGE_BYTES + 1), 1)
            .await
            .is_none()
    );
    let input = admission
        .admit("é".repeat(MAX_MESSAGE_BYTES / 2), 1)
        .await
        .unwrap();
    assert_eq!(
        admission.0.available_permits(),
        MAX_ADMITTED_BYTES - MAX_MESSAGE_BYTES - ENTRY_RESERVE_BYTES
    );
    drop(input);
    assert_eq!(admission.0.available_permits(), MAX_ADMITTED_BYTES);
}

#[tokio::test]
async fn stale_generation_is_rejected_even_when_sequence_is_reused() {
    let admission = Admission::default();
    let input = admission
        .admit(r#"{"seq":1,"press":"nav.next"}"#.into(), 7)
        .await
        .unwrap();
    assert!(input.decode(8).is_none());
    assert_eq!(input.decode(7).unwrap().seq, 1);
}

#[tokio::test]
async fn full_browser_budget_does_not_block_daemon_lifecycle_admission() {
    let admission = Admission::default();
    let reservation = admission
        .0
        .clone()
        .acquire_many_owned(MAX_ADMITTED_BYTES as u32)
        .await
        .unwrap();
    let pending = admission.admit(r#"{"seq":1}"#.into(), 1);
    tokio::pin!(pending);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut pending)
            .await
            .is_err()
    );
    let (tell, inbox) = std::sync::mpsc::channel();
    tell.send(crate::setup::Heard::Gone(1, "closed".into()))
        .unwrap();
    assert!(matches!(
        inbox.try_recv().unwrap(),
        crate::setup::Heard::Gone(1, _)
    ));
    drop(reservation);
    let input = pending.await.unwrap();
    assert_eq!(input.decode(1).unwrap().seq, 1);
}

#[tokio::test]
async fn cancelled_waiter_releases_its_fifo_position_and_all_accounting() {
    let admission = Admission::default();
    let held = admission
        .0
        .clone()
        .acquire_many_owned(MAX_ADMITTED_BYTES as u32)
        .await
        .unwrap();
    let mut first = Box::pin(admission.admit("large".repeat(100), 1));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut first)
            .await
            .is_err()
    );
    let mut next = Box::pin(admission.admit(r#"{"seq":1}"#.into(), 1));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut next)
            .await
            .is_err()
    );
    drop(first);
    drop(held);
    let input = tokio::time::timeout(std::time::Duration::from_secs(1), next)
        .await
        .unwrap()
        .unwrap();
    assert!(input.decode(1).is_some());
    drop(input);
    assert_eq!(admission.0.available_permits(), MAX_ADMITTED_BYTES);
}
