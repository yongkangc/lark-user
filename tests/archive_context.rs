mod support;
use chrono::Duration;
use lark_user::{
    ErrorCode,
    archive::{Archive, MessageQuery},
    context,
};
use support::*;

fn query(chat: Option<&str>, limit: usize) -> MessageQuery {
    MessageQuery {
        chat: chat.map(str::to_owned),
        limit,
        ..Default::default()
    }
}

#[test]
fn cursors_reject_changed_chat_limits_and_other_archives_with_the_same_scope() {
    let directory = private_tempdir();
    let mut first =
        Archive::open_or_create(&directory.path().join("first.sqlite"), scope()).unwrap();
    let mut second =
        Archive::open_or_create(&directory.path().join("second.sqlite"), scope()).unwrap();
    for archive in [&mut first, &mut second] {
        archive
            .import(
                vec![message("a", "one", 0), message("b", "two", 1)],
                None,
                now(),
            )
            .unwrap();
    }
    let chats = first.chats(1, None).unwrap();
    let cursor = chats.next_cursor.as_deref();
    assert_eq!(first.chats(1, cursor).unwrap().items[0].chat_id, "two");
    assert_eq!(
        first.chats(2, cursor).unwrap_err().code,
        ErrorCode::InvalidCursor
    );
    assert_eq!(
        second.chats(1, cursor).unwrap_err().code,
        ErrorCode::InvalidCursor
    );
    let mut next = query(None, 1);
    next.cursor = first.messages(&next).unwrap().next_cursor;
    assert_eq!(first.messages(&next).unwrap().items[0].message_id, "a");
    assert_eq!(
        second.messages(&next).unwrap_err().code,
        ErrorCode::InvalidCursor
    );
    next.limit = 2;
    assert_eq!(
        first.messages(&next).unwrap_err().code,
        ErrorCode::InvalidCursor
    );
}

#[test]
fn stable_pagination_handles_equal_times_and_cross_chat_message_ids() {
    let directory = private_tempdir();
    let mut archive =
        Archive::open_or_create(&directory.path().join("archive.sqlite"), scope()).unwrap();
    archive
        .import(
            vec![
                message("a", "one", 0),
                message("b", "one", 0),
                message("a", "two", 0),
                message("b", "two", 0),
            ],
            None,
            now(),
        )
        .unwrap();
    let first = archive.messages(&query(None, 2)).unwrap();
    assert!(first.has_more);
    let mut next = query(None, 2);
    next.cursor = first.next_cursor.clone();
    let second = archive.messages(&next).unwrap();
    assert_eq!(second.items.len(), 2);
    assert!(!second.has_more);
    let identities = first
        .items
        .iter()
        .chain(&second.items)
        .map(|m| (&m.chat_id, &m.message_id))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(), 4);
    next.chat = Some("one".into());
    assert_eq!(
        archive.messages(&next).unwrap_err().code,
        ErrorCode::InvalidCursor
    );
    assert_eq!(
        archive.get_message("a").unwrap_err().code,
        ErrorCode::AmbiguousId
    );
    assert_eq!(
        archive.messages(&query(None, 0)).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
}

#[test]
fn deduplicates_preserves_newer_revisions_and_rolls_back_conflicts() {
    let directory = private_tempdir();
    let mut archive =
        Archive::open_or_create(&directory.path().join("archive.sqlite"), scope()).unwrap();
    let original = message("a", "one", 0);
    assert_eq!(
        archive
            .import(vec![original.clone(), original.clone()], None, now())
            .unwrap()
            .duplicates,
        1
    );
    let mut edited = original.clone();
    edited.edited_at = Some(now() + Duration::minutes(2));
    edited.text = Some("Synthetic newer revision".into());
    assert_eq!(
        archive
            .import(vec![edited.clone()], None, now())
            .unwrap()
            .updated,
        1
    );
    assert_eq!(
        archive.import(vec![original], None, now()).unwrap().stale,
        1
    );
    assert_eq!(
        archive.get_message("a").unwrap().text.as_deref(),
        Some("Synthetic newer revision")
    );
    edited.text = Some("Synthetic equal-revision conflict".into());
    let error = archive
        .import(vec![message("new", "one", 0), edited], None, now())
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ArchiveConflict);
    assert_eq!(
        archive.get_message("new").unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[test]
fn scope_mismatches_reject_imports_and_reopening_another_tenants_archive() {
    let directory = private_tempdir();
    let path = directory.path().join("archive.sqlite");
    let mut archive = Archive::open_or_create(&path, scope()).unwrap();
    let mut other = message("other", "one", 0);
    other.scope.tenant_id = "other-synthetic-tenant".into();
    assert_eq!(
        archive
            .import(vec![message("a", "one", 0), other], None, now())
            .unwrap_err()
            .code,
        ErrorCode::ScopeConflict
    );
    assert!(archive.messages(&query(None, 10)).unwrap().items.is_empty());
    drop(archive);
    let mut other_scope = scope();
    other_scope.account_id = "other-synthetic-account".into();
    let error = Archive::open_existing(&path, other_scope).err().unwrap();
    assert_eq!(error.code, ErrorCode::ScopeConflict);
}

#[test]
fn local_search_is_literal_and_tombstones_remove_text_and_payload() {
    let directory = private_tempdir();
    let path = directory.path().join("archive.sqlite");
    let mut archive = Archive::open_or_create(&path, scope()).unwrap();
    let mut record = message("a", "one", 0);
    record.text = Some("SYNTHETIC_ERASABLE_CONTENT marker".into());
    record.structured_payload = Some(serde_json::json!({"generic":"SYNTHETIC_ERASABLE_CONTENT"}));
    archive.import(vec![record.clone()], None, now()).unwrap();
    let mut search = query(None, 10);
    search.search = Some("marker".into());
    assert_eq!(archive.messages(&search).unwrap().items.len(), 1);
    search.search = Some("absent OR marker*".into());
    assert!(archive.messages(&search).unwrap().items.is_empty());
    record.deleted = true;
    record.updated_at = Some(now() + Duration::minutes(1));
    archive.import(vec![record], None, now()).unwrap();
    search.search = Some("marker".into());
    assert!(archive.messages(&search).unwrap().items.is_empty());
    let tombstone = archive.get_message("a").unwrap();
    assert!(tombstone.text.is_none() && tombstone.structured_payload.is_none());
    let bytes = std::fs::read(path).unwrap();
    assert!(
        !bytes
            .windows(b"SYNTHETIC_ERASABLE_CONTENT".len())
            .any(|v| v == b"SYNTHETIC_ERASABLE_CONTENT")
    );
}

#[test]
fn retention_prune_clear_and_coverage_stay_explicit() {
    let directory = private_tempdir();
    let path = directory.path().join("archive.sqlite");
    let mut archive = Archive::open_or_create(&path, scope()).unwrap();
    let report = archive
        .import(
            vec![message("old", "one", -30), message("new", "one", 0)],
            Some(now() - Duration::minutes(1)),
            now(),
        )
        .unwrap();
    assert_eq!(report.outside_retention, 1);
    assert_eq!(
        archive.coverage(Some("one")).unwrap()[0].retained_message_count,
        1
    );
    assert!(!archive.coverage(None).unwrap()[0].contiguous_history_verified);
    assert_eq!(
        archive.prune(now() + Duration::seconds(1), now()).unwrap(),
        1
    );
    assert!(archive.messages(&query(None, 10)).unwrap().items.is_empty());
    archive
        .import(vec![message("again", "one", 0)], None, now())
        .unwrap();
    assert_eq!(archive.clear(now()).unwrap(), 1);
    assert!(archive.coverage(None).unwrap().is_empty());
    assert!(archive.chats(10, None).unwrap().items.is_empty());
}

#[test]
fn fixture_round_trip_includes_mentions_quotes_attachments_and_unsupported_payload() {
    let directory = private_tempdir();
    let mut archive =
        Archive::open_or_create(&directory.path().join("archive.sqlite"), scope()).unwrap();
    let mut records = Archive::parse_jsonl(include_bytes!("fixtures/normalized.jsonl")).unwrap();
    records[0].source_link_verified = true;
    records[0].quote = Some(lark_user::model::Quote {
        message_id: Some("synthetic-quote-001".into()),
        text: Some("Generic quoted content".into()),
    });
    records[0].attachments.push(lark_user::model::Attachment {
        id: Some("synthetic-file-001".into()),
        name: Some("generic.txt".into()),
        media_type: Some("text/plain".into()),
        size_bytes: Some(42),
    });
    records[0].structured_payload = Some(serde_json::json!({"synthetic_unknown_content":[1,2,3]}));
    archive.import(records, None, now()).unwrap();
    let record = archive.get_message("synthetic-message-001").unwrap();
    assert_eq!(record.mentions.len(), 1);
    assert_eq!(record.attachments[0].size_bytes, Some(42));
    assert_eq!(
        record.quote.unwrap().message_id.as_deref(),
        Some("synthetic-quote-001")
    );
    assert!(record.structured_payload.is_some());
    assert!(!record.source_link_verified);
    let mut thread = query(None, 10);
    thread.thread = Some("synthetic-thread-001".into());
    assert_eq!(archive.messages(&thread).unwrap().items.len(), 2);
}

#[test]
fn context_is_bounded_chronological_attributed_and_untrusted() {
    let directory = private_tempdir();
    let mut archive =
        Archive::open_or_create(&directory.path().join("archive.sqlite"), scope()).unwrap();
    let mut huge = message("huge", "one", 3);
    huge.text = Some("Synthetic untrusted content 界".repeat(1000));
    archive
        .import(
            vec![message("a", "one", 0), message("b", "one", 1), huge],
            None,
            now(),
        )
        .unwrap();
    let bundle = context::generate(&archive, &query(Some("one"), 10), 4096, now()).unwrap();
    assert!(serde_json::to_vec(&bundle).unwrap().len() <= 4096);
    assert_eq!(bundle.omitted_for_budget, 1);
    assert_eq!(bundle.messages.len(), 2);
    assert_eq!(bundle.messages[0].message.message_id, "a");
    assert_eq!(bundle.messages[1].source.message_id, "b");
    assert!(
        bundle
            .messages
            .iter()
            .all(|m| m.trust == "untrusted_message_data")
    );
    assert!(!bundle.completeness.complete);
    assert!(bundle.content_policy.contains("Explicit commitments"));
    assert!(context::generate(&archive, &query(Some("one"), 10), 10, now()).is_err());
}
