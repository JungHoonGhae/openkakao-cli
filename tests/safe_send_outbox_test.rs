use openkakao_cli::safe_send::{
    ProposeSend, SafeSendLimits, SafeSendOutbox, SendIntent, SendState, SendTransport,
    TransportOutcome,
};

#[derive(Default)]
struct RecordingTransport {
    sends: Vec<(String, String)>,
}

struct FailingTransport {
    attempts: usize,
}

struct NotSentTransport {
    attempts: usize,
}

struct PanickingTransport;

impl SendTransport for PanickingTransport {
    fn send(&mut self, _chat_name: &str, _message: &str) -> TransportOutcome {
        panic!("simulated process interruption after send claim")
    }
}

struct ConcurrentInspector {
    path: std::path::PathBuf,
    intent_id: String,
    observed: Option<SendState>,
}

impl SendTransport for ConcurrentInspector {
    fn send(&mut self, _chat_name: &str, _message: &str) -> TransportOutcome {
        let observation =
            SafeSendOutbox::open_at(&self.path).and_then(|observer| observer.get(&self.intent_id));
        match observation {
            Ok(intent) => {
                self.observed = Some(intent.state);
                TransportOutcome::Verified
            }
            Err(error) => TransportOutcome::NotSent {
                reason: error.to_string(),
            },
        }
    }
}

impl SendTransport for FailingTransport {
    fn send(&mut self, _chat_name: &str, _message: &str) -> TransportOutcome {
        self.attempts += 1;
        TransportOutcome::Uncertain {
            reason: "AX result is ambiguous".to_string(),
        }
    }
}

impl SendTransport for NotSentTransport {
    fn send(&mut self, _chat_name: &str, _message: &str) -> TransportOutcome {
        self.attempts += 1;
        TransportOutcome::NotSent {
            reason: "target window was not found before commit".to_string(),
        }
    }
}

impl SendTransport for RecordingTransport {
    fn send(&mut self, chat_name: &str, message: &str) -> TransportOutcome {
        self.sends
            .push((chat_name.to_string(), message.to_string()));
        TransportOutcome::Verified
    }
}

fn propose_manual(outbox: &SafeSendOutbox, chat_name: &str, message: &str) -> SendIntent {
    outbox
        .propose(ProposeSend {
            chat_name,
            message,
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap()
}

fn assert_second_global_send_is_blocked(limits: SafeSendLimits, expected_error: &str) {
    let dir = tempfile::tempdir().unwrap();
    let mut outbox =
        SafeSendOutbox::open_at_with_limits(&dir.path().join("safe-send.db"), limits).unwrap();
    let first = propose_manual(&outbox, "첫 번째 방", "첫 번째");
    let second = propose_manual(&outbox, "두 번째 방", "두 번째");
    let mut transport = RecordingTransport::default();

    outbox
        .approve_and_send(&first.intent_id, &first.approval_code, &mut transport)
        .unwrap();
    let error = outbox
        .approve_and_send(&second.intent_id, &second.approval_code, &mut transport)
        .unwrap_err();

    assert!(
        error.to_string().contains(expected_error),
        "unexpected error: {error:#}"
    );
    assert_eq!(transport.sends.len(), 1);
    assert_eq!(
        outbox.get(&second.intent_id).unwrap().state,
        SendState::Proposed
    );
}

#[test]
fn proposal_is_persisted_without_sending_and_deduplicated_by_source_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("safe-send.db");
    let outbox = SafeSendOutbox::open_at(&path).unwrap();

    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "검토할 답장",
            reply_to: Some((42, 99)),
            idempotency_key: Some("reply:42:99:policy-v1"),
        })
        .unwrap();

    assert_eq!(proposed.state, SendState::Proposed);
    assert_eq!(proposed.chat_name, "허용된 방");
    assert_eq!(proposed.message, "검토할 답장");
    assert_eq!(proposed.reply_to, Some((42, 99)));
    assert_eq!(proposed.approval_code.len(), 12);

    let duplicate = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "검토할 답장",
            reply_to: Some((42, 99)),
            idempotency_key: Some("reply:42:99:policy-v1"),
        })
        .unwrap();
    assert_eq!(duplicate.intent_id, proposed.intent_id);

    drop(outbox);
    let reopened = SafeSendOutbox::open_at(&path).unwrap();
    assert_eq!(reopened.list_active().unwrap(), vec![proposed]);
}

#[test]
fn idempotency_key_rejects_different_proposal_content() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = SafeSendOutbox::open_at(&dir.path().join("safe-send.db")).unwrap();
    let original = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "검토할 답장",
            reply_to: Some((42, 99)),
            idempotency_key: Some("reply:42:99:policy-v1"),
        })
        .unwrap();

    for conflicting in [
        ProposeSend {
            chat_name: "다른 방",
            message: "검토할 답장",
            reply_to: Some((42, 99)),
            idempotency_key: Some("reply:42:99:policy-v1"),
        },
        ProposeSend {
            chat_name: "허용된 방",
            message: "다른 답장",
            reply_to: Some((42, 99)),
            idempotency_key: Some("reply:42:99:policy-v1"),
        },
        ProposeSend {
            chat_name: "허용된 방",
            message: "검토할 답장",
            reply_to: Some((42, 100)),
            idempotency_key: Some("reply:42:99:policy-v1"),
        },
    ] {
        let error = outbox.propose(conflicting).unwrap_err();
        assert!(error.to_string().contains("different content"));
    }

    assert_eq!(outbox.list_active().unwrap(), vec![original]);
}

#[test]
fn matching_approval_code_sends_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut outbox = SafeSendOutbox::open_at(&dir.path().join("safe-send.db")).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "한 번만 전송",
            reply_to: Some((42, 100)),
            idempotency_key: Some("reply:42:100:policy-v1"),
        })
        .unwrap();
    let mut transport = RecordingTransport::default();

    let mut wrong_code = proposed.approval_code.clone().into_bytes();
    wrong_code[0] = if wrong_code[0] == b'0' { b'1' } else { b'0' };
    let wrong_code = String::from_utf8(wrong_code).unwrap();
    assert!(outbox
        .approve_and_send(&proposed.intent_id, &wrong_code, &mut transport)
        .is_err());
    assert!(transport.sends.is_empty());

    let sent = outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport)
        .unwrap();

    assert_eq!(sent.state, SendState::Sent);
    assert_eq!(
        transport.sends,
        vec![("허용된 방".to_string(), "한 번만 전송".to_string())]
    );
    assert!(outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport,)
        .is_err());
    assert_eq!(transport.sends.len(), 1);
}

#[test]
fn ambiguous_transport_failure_is_quarantined_and_never_auto_retried() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("safe-send.db");
    let mut outbox = SafeSendOutbox::open_at(&path).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "중복되면 안 됨",
            reply_to: Some((42, 101)),
            idempotency_key: Some("reply:42:101:policy-v1"),
        })
        .unwrap();
    let mut transport = FailingTransport { attempts: 0 };

    let error = outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport)
        .unwrap_err();
    assert_eq!(transport.attempts, 1);
    // Agents branch on the code: an uncertain approval must say so.
    assert_eq!(
        openkakao_cli::error::error_json(&error)["error"]["code"],
        "delivery_uncertain"
    );
    drop(outbox);

    let mut reopened = SafeSendOutbox::open_at(&path).unwrap();
    let active = reopened.list_active().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].state, SendState::Uncertain);
    assert!(active[0]
        .last_error
        .as_deref()
        .unwrap()
        .contains("ambiguous"));
    assert!(reopened
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport,)
        .is_err());
    assert_eq!(transport.attempts, 1);
}

#[test]
fn definitely_not_sent_returns_to_proposed_and_can_be_approved_again() {
    let dir = tempfile::tempdir().unwrap();
    let limits = SafeSendLimits {
        min_interval_secs: 0,
        ..SafeSendLimits::default()
    };
    let mut outbox =
        SafeSendOutbox::open_at_with_limits(&dir.path().join("safe-send.db"), limits).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "아직 전송되지 않음",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap();
    let mut not_sent = NotSentTransport { attempts: 0 };

    let error = outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut not_sent)
        .unwrap_err();

    assert!(error.to_string().contains("was not sent"));
    assert_eq!(not_sent.attempts, 1);
    let retryable = outbox.get(&proposed.intent_id).unwrap();
    assert_eq!(retryable.state, SendState::Proposed);
    assert!(retryable
        .last_error
        .as_deref()
        .unwrap()
        .contains("target window"));

    let mut verified = RecordingTransport::default();
    let sent = outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut verified)
        .unwrap();
    assert_eq!(sent.state, SendState::Sent);
    assert_eq!(verified.sends.len(), 1);
}

#[test]
fn interrupted_execution_becomes_uncertain_when_the_outbox_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("safe-send.db");
    let limits = SafeSendLimits {
        sending_lease_secs: 0,
        ..SafeSendLimits::default()
    };
    let mut outbox = SafeSendOutbox::open_at_with_limits(&path, limits).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "중단 테스트",
            reply_to: Some((42, 102)),
            idempotency_key: Some("reply:42:102:policy-v1"),
        })
        .unwrap();

    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = outbox.approve_and_send(
            &proposed.intent_id,
            &proposed.approval_code,
            &mut PanickingTransport,
        );
    }));
    assert!(interrupted.is_err());
    drop(outbox);

    let reopened = SafeSendOutbox::open_at_with_limits(&path, limits).unwrap();
    let active = reopened.list_active().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].state, SendState::Uncertain);
    assert!(active[0]
        .last_error
        .as_deref()
        .unwrap()
        .contains("interrupted"));
}

#[test]
fn cancelled_proposal_can_never_be_sent() {
    let dir = tempfile::tempdir().unwrap();
    let mut outbox = SafeSendOutbox::open_at(&dir.path().join("safe-send.db")).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "취소할 메시지",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap();
    let mut transport = RecordingTransport::default();

    let cancelled = outbox.cancel(&proposed.intent_id).unwrap();

    assert_eq!(cancelled.state, SendState::Cancelled);
    assert!(outbox.list_active().unwrap().is_empty());
    assert!(outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport,)
        .is_err());
    assert!(transport.sends.is_empty());
}

#[test]
fn configured_send_budget_refuses_excess_messages_before_transport() {
    let dir = tempfile::tempdir().unwrap();
    let limits = SafeSendLimits {
        min_interval_secs: 0,
        max_per_chat_per_hour: 1,
        max_global_per_hour: 10,
        max_global_per_day: 10,
        sending_lease_secs: 5 * 60,
        proposal_ttl_secs: 15 * 60,
    };
    let mut outbox =
        SafeSendOutbox::open_at_with_limits(&dir.path().join("safe-send.db"), limits).unwrap();
    let first = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "첫 번째",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap();
    let second = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "두 번째",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap();
    let mut transport = RecordingTransport::default();
    outbox
        .approve_and_send(&first.intent_id, &first.approval_code, &mut transport)
        .unwrap();

    let error = outbox
        .approve_and_send(&second.intent_id, &second.approval_code, &mut transport)
        .unwrap_err();

    assert!(error.to_string().contains("per-chat hourly budget"));
    assert_eq!(transport.sends.len(), 1);
    assert_eq!(outbox.list_active().unwrap(), vec![second]);
}

#[test]
fn global_cooldown_refuses_a_second_send_before_transport() {
    assert_second_global_send_is_blocked(
        SafeSendLimits {
            min_interval_secs: 60,
            ..SafeSendLimits::default()
        },
        "global cooldown",
    );
}

#[test]
fn global_hourly_budget_refuses_a_second_send_before_transport() {
    assert_second_global_send_is_blocked(
        SafeSendLimits {
            min_interval_secs: 0,
            max_per_chat_per_hour: 10,
            max_global_per_hour: 1,
            max_global_per_day: 10,
            ..SafeSendLimits::default()
        },
        "global hourly budget",
    );
}

#[test]
fn global_daily_budget_refuses_a_second_send_before_transport() {
    assert_second_global_send_is_blocked(
        SafeSendLimits {
            min_interval_secs: 0,
            max_per_chat_per_hour: 10,
            max_global_per_hour: 10,
            max_global_per_day: 1,
            ..SafeSendLimits::default()
        },
        "global daily budget",
    );
}

#[test]
fn concurrent_inspection_does_not_quarantine_an_in_flight_send() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("safe-send.db");
    let mut outbox = SafeSendOutbox::open_at(&path).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "진행 중 관찰",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap();
    let mut transport = ConcurrentInspector {
        path: path.clone(),
        intent_id: proposed.intent_id.clone(),
        observed: None,
    };

    outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport)
        .unwrap();

    assert_eq!(transport.observed, Some(SendState::Sending));
    assert_eq!(
        outbox.get(&proposed.intent_id).unwrap().state,
        SendState::Sent
    );
}

#[test]
fn proposal_rejects_terminal_control_sequences() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = SafeSendOutbox::open_at(&dir.path().join("safe-send.db")).unwrap();

    let error = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "정상처럼 보임\u{1b}[2J위조된 승인 화면",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap_err();

    assert!(error.to_string().contains("control character"));
    assert!(outbox.list_active().unwrap().is_empty());
}

#[test]
fn proposal_rejects_bidirectional_display_spoofing() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = SafeSendOutbox::open_at(&dir.path().join("safe-send.db")).unwrap();

    for dangerous in [
        '\u{061c}', '\u{200e}', '\u{200f}', '\u{202e}', '\u{2066}', '\u{2069}',
    ] {
        let message = format!("review {dangerous}spoofed");
        let error = outbox
            .propose(ProposeSend {
                chat_name: "허용된 방",
                message: &message,
                reply_to: None,
                idempotency_key: None,
            })
            .unwrap_err();
        assert!(error.to_string().contains("bidirectional"));
    }
    assert!(outbox.list_active().unwrap().is_empty());
}

#[test]
fn expired_proposal_is_never_claimed_for_transport() {
    let dir = tempfile::tempdir().unwrap();
    let limits = SafeSendLimits {
        proposal_ttl_secs: 0,
        ..SafeSendLimits::default()
    };
    let mut outbox =
        SafeSendOutbox::open_at_with_limits(&dir.path().join("safe-send.db"), limits).unwrap();
    let proposed = outbox
        .propose(ProposeSend {
            chat_name: "허용된 방",
            message: "이미 만료",
            reply_to: None,
            idempotency_key: None,
        })
        .unwrap();
    let mut transport = RecordingTransport::default();

    let error = outbox
        .approve_and_send(&proposed.intent_id, &proposed.approval_code, &mut transport)
        .unwrap_err();

    assert!(error.to_string().contains("expired"));
    assert!(transport.sends.is_empty());
    assert!(outbox.list_active().unwrap().is_empty());
    assert_eq!(
        outbox.get(&proposed.intent_id).unwrap().state,
        SendState::Expired
    );
}
