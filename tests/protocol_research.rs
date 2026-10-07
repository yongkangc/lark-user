use lark_user::{
    ErrorCode,
    protocol::{self, Operation},
};

#[test]
fn passive_har_inspection_preserves_observations_without_secrets_or_schema_claims() {
    let har = serde_json::json!({"log":{"entries":[{
        "request":{"method":"POST","url":"https://regional.example.test/im/gateway/?ticket=SYNTHETIC_TICKET","headers":[
            {"name":"Cookie","value":"session=SYNTHETIC_COOKIE; preference=SYNTHETIC_PREF"},
            {"name":"Authorization","value":"Bearer SYNTHETIC_AUTH"},
            {"name":"x-csrf-token","value":"SYNTHETIC_CSRF"},
            {"name":"x-command","value":"11021"}],"postData":{"text":"SYNTHETIC_BODY"}},
        "response":{"status":429,"headers":[{"name":"Retry-After","value":"5"},{"name":"Set-Cookie","value":"session=SYNTHETIC_ROTATION"}],"content":{"mimeType":"application/x-protobuf","text":"SYNTHETIC_RESPONSE"}}
    },{
        "request":{"method":"GET","url":"https://tenant.example.test/private/SYNTHETIC_PATH?token=SYNTHETIC_QUERY#SYNTHETIC_FRAGMENT","headers":[]},
        "response":{"status":200,"content":{"mimeType":"application/json","text":"SYNTHETIC_MESSAGE"}}
    }]}});
    let report = protocol::inspect_har(&serde_json::to_vec(&har).unwrap()).unwrap();
    let output = serde_json::to_string(&report).unwrap();
    assert!(!output.contains("SYNTHETIC_"));
    assert_eq!(report.origins.len(), 2);
    assert_eq!(report.observations[0].numeric_command_id, Some(11021));
    assert_eq!(report.observations[0].retry_after_seconds, Some(5));
    assert!(report.observations[0].set_cookie_seen);
    assert!(
        report.observations[0]
            .additional_session_header_names
            .contains("x-csrf-token")
    );
    assert!(!report.live_protocol_verified);
}

#[test]
fn live_operations_fail_closed_instead_of_using_feishu_hosts() {
    for operation in [
        Operation::Chats,
        Operation::Messages,
        Operation::Search,
        Operation::Threads,
        Operation::Sync,
        Operation::Send,
        Operation::Reply,
        Operation::Watch,
    ] {
        let error = protocol::require_live(operation).unwrap_err();
        assert_eq!(error.code, ErrorCode::ProtocolUnverified);
        assert_eq!(error.exit_code(), 5);
        assert!(!error.message.contains("feishu.cn"));
    }
}
