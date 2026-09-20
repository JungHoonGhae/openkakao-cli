use bson::{doc, Bson, Document};
use openkakao_cli::loco::{message::received_messages, packet::LocoPacket};

// Synthetic protocol examples, containing no captured account/message data.
fn packet(method: &str, body: Document) -> LocoPacket {
    LocoPacket::decode(
        &LocoPacket {
            packet_id: 7,
            status_code: 0,
            method: method.into(),
            body_type: 0,
            body,
        }
        .encode(),
    )
    .unwrap()
}

fn log() -> Document {
    doc! {
        "chatId": 101_i64,
        "logId": 9007199254740993_i64,
        "prevId": 9007199254740992_i64,
        "authorId": 202_i64,
        "type": 1_i32,
        "message": "안녕하세요 🦀",
        "sendAt": 1788760000_i32,
        "attachment": "",
    }
}

#[test]
fn nested_msg_preserves_identity_text_and_outer_nickname() {
    let packet = packet(
        "MSG",
        doc! {
            "chatId": 101_i64,
            "authorNickname": "테스터",
            "chatLog": log(),
            // Outer lookalikes must never replace the actual nested log.
            "logId": 999_i64, "type": 2, "msg": "wrong",
        },
    );
    let messages = received_messages(&packet).unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.chat_id, 101);
    assert_eq!(message.log_id, 9007199254740993);
    assert_eq!(message.author_id, 202);
    assert_eq!(message.message_type, 1);
    assert_eq!(message.send_at, 1788760000);
    assert_eq!(message.nickname, "테스터");
    assert_eq!(message.body.get_str("message").unwrap(), "안녕하세요 🦀");
}

#[test]
fn flat_legacy_msg_and_integer_widths_remain_supported() {
    let packet = packet(
        "MSG",
        doc! {
            "chatId": 101_i32, "logId": 12_i32, "authorId": 202_i32,
            "type": 1_i64, "msg": "legacy", "sendAt": 1788760000_i64,
            "author": {"nickName": "Legacy"},
        },
    );
    let messages = received_messages(&packet).unwrap();
    let message = &messages[0];
    assert_eq!(message.nickname, "Legacy");
    assert_eq!(message.log_id, 12);
    assert_eq!(message.author_id, 202);
    assert_eq!(message.send_at, 1788760000);
    assert_eq!(message.body.get_str("msg").unwrap(), "legacy");
}

#[test]
fn syncmsg_decodes_each_log_and_empty_batches_emit_nothing() {
    let mut second = log();
    second.insert("logId", 12_i32);
    second.insert("chatId", 303_i64);
    let packet = packet("SYNCMSG", doc! {"isOK": true, "chatLogs": [log(), second]});
    let messages = received_messages(&packet).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].chat_id, 101);
    assert_eq!(messages[1].chat_id, 303);
    assert_eq!(messages[1].log_id, 12);
    let empty = packet_with_empty_batch();
    assert!(received_messages(&empty).unwrap().is_empty());
}

fn packet_with_empty_batch() -> LocoPacket {
    packet("SYNCMSG", doc! {"isOK": true, "chatLogs": []})
}

#[test]
fn syncmsg_flat_and_single_nested_compatibility() {
    for body in [log(), doc! {"chatLog": log()}] {
        let packet = packet("SYNCMSG", body);
        assert_eq!(received_messages(&packet).unwrap()[0].chat_id, 101);
    }
}

#[test]
fn envelope_chat_id_fills_only_an_absent_inner_chat_id() {
    let mut inner = log();
    inner.remove("chatId");
    let valid = packet("MSG", doc! {"chatId": 101_i64, "chatLog": inner.clone()});
    assert_eq!(received_messages(&valid).unwrap()[0].chat_id, 101);
    for bad_id in [
        Bson::Int64(303),
        Bson::Int32(0),
        Bson::String("101".into()),
        Bson::Null,
    ] {
        inner.insert("chatId", bad_id);
        let invalid = packet("MSG", doc! {"chatId": 101_i64, "chatLog": inner.clone()});
        assert!(received_messages(&invalid).is_err());
    }
}

#[test]
fn malformed_nested_payload_never_falls_back_to_outer_fields() {
    for value in [
        Bson::Null,
        Bson::String("bad".into()),
        Bson::Array(vec![]),
        Bson::Document(doc! {}),
    ] {
        let mut body = log();
        body.insert("chatLog", value);
        let packet = packet("MSG", body);
        assert!(received_messages(&packet).is_err());
    }
}

#[test]
fn malformed_batch_is_rejected_before_any_records_are_returned() {
    for value in [
        Bson::Null,
        Bson::String("bad".into()),
        Bson::Document(doc! {}),
    ] {
        let packet = packet("SYNCMSG", doc! {"chatLogs": [Bson::Document(log()), value]});
        assert!(received_messages(&packet).is_err());
    }
    let packet = packet("SYNCMSG", doc! {"chatLogs": {}, "chatLog": log()});
    assert!(received_messages(&packet).is_err());
}

#[test]
fn missing_identity_and_out_of_range_type_are_rejected() {
    for key in ["chatId", "logId", "type"] {
        let mut body = log();
        body.remove(key);
        assert!(received_messages(&packet("MSG", body)).is_err());
    }
    for (key, value) in [("logId", 0_i64), ("logId", -1), ("type", i64::MAX)] {
        let mut body = log();
        body.insert(key, value);
        assert!(received_messages(&packet("MSG", body)).is_err());
    }
}

#[test]
fn error_responses_and_other_packet_methods_are_not_messages() {
    let mut body = log();
    body.insert("status", -950_i32);
    assert!(received_messages(&packet("SYNCMSG", body)).is_err());
    let mut failed = packet("MSG", log());
    failed.status_code = -1;
    assert!(received_messages(&failed).is_err());
    assert!(received_messages(&packet("PING", log())).is_err());
}

#[test]
fn nested_media_uses_inner_attachment() {
    let mut body = log();
    body.insert("type", 26);
    body.insert("attachment", r#"{"name":"example.txt","s":1024}"#);
    let packet = packet("MSG", doc! {"chatLog": body, "attachment": "wrong"});
    let messages = received_messages(&packet).unwrap();
    let message = &messages[0];
    assert_eq!(message.attachment, r#"{"name":"example.txt","s":1024}"#);
}
