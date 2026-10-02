use super::*;
const T: i64 = 1_780_000_000;
#[test]
fn refresh_cannot_reset_inflight_learning_and_repeated_restore_keeps_tokens_bounded() {
    let s = store();
    let e = ready(&s, "architecture");
    let e = s.generation_begin(e.id, e.revision, true).unwrap();
    assert!(s
        .review(e.id, e.revision, "blocked-study", "study", T)
        .is_err());
    let fresh = s
        .review(e.id, e.revision, "restart-0001", "restart", T)
        .unwrap();
    assert!(s
        .generation_finish(
            e.id,
            e.content_revision,
            Ok(fixture_card("architecture")),
            "test",
            "test",
            T
        )
        .is_err());
    assert_eq!(fresh.status, "new");
    let mut json = s.export().unwrap();
    for _ in 0..12 {
        let restored = store();
        restored.import(&json, true, T).unwrap();
        json = restored.export().unwrap();
    }
}
fn store() -> VocabularyStore {
    VocabularyStore::from_connection(Connection::open_in_memory().unwrap()).unwrap()
}
fn ready(s: &VocabularyStore, word: &str) -> Entry {
    let e = s.collect(word, "", "", T).unwrap();
    s.save_card(e.id, e.revision, fixture_card(word), T)
        .unwrap()
}
fn study(s: &VocabularyStore, e: Entry) -> Entry {
    s.review(e.id, e.revision, "study-0001", "study", T)
        .unwrap()
}
fn at_test(s: &VocabularyStore) -> Entry {
    let mut e = study(s, ready(s, "architecture"));
    for i in 1..=3 {
        e = s
            .review(
                e.id,
                e.revision,
                &format!("review-000{i}"),
                "remember",
                T + MIN_INTERVAL * i,
            )
            .unwrap();
    }
    e
}
#[test]
fn normalization_duplicate_context_and_translation_clear_are_independent() {
    let s = store();
    let a = s
        .collect(
            "Architecture",
            "She studies architecture.",
            "她学习建筑学。",
            T,
        )
        .unwrap();
    let b = s
        .collect("architecture", "I like architecture.", "", T)
        .unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(b.sources.len(), 2);
    assert_eq!(s.list("arch", "learning", 0, T).unwrap().total, 1);
    for invalid in ["two words", "<script>", "-run", "can't!", ""] {
        assert!(normalize_word(invalid).is_err());
    }
}
#[test]
fn initial_learning_and_exact_four_hour_boundary() {
    let s = store();
    let e = ready(&s, "architecture");
    assert_eq!(e.reviews, 0);
    let e = study(&s, e);
    assert!(s
        .review(
            e.id,
            e.revision,
            "early-0001",
            "remember",
            T + MIN_INTERVAL - 1
        )
        .is_err());
    let r = s
        .review(
            e.id,
            e.revision,
            "review-0001",
            "remember",
            T + MIN_INTERVAL,
        )
        .unwrap();
    assert_eq!(r.reviews, 1);
    let duplicate = s
        .review(
            e.id,
            e.revision,
            "review-0001",
            "remember",
            T + MIN_INTERVAL + 1,
        )
        .unwrap();
    assert_eq!(duplicate.reviews, 1);
    assert!(s
        .review(
            e.id,
            e.revision,
            "review-0002",
            "remember",
            T + MIN_INTERVAL + 2
        )
        .is_err());
}
#[test]
fn fuzzy_forgot_and_early_view_do_not_grant_mastery() {
    let s = store();
    let mut e = study(&s, ready(&s, "architecture"));
    e = s
        .review(
            e.id,
            e.revision,
            "review-0001",
            "remember",
            T + MIN_INTERVAL,
        )
        .unwrap();
    let before = s.get(e.id).unwrap();
    assert_eq!(before.reviews, 1);
    e = s
        .review(
            e.id,
            e.revision,
            "review-0002",
            "fuzzy",
            T + 2 * MIN_INTERVAL,
        )
        .unwrap();
    assert_eq!(e.reviews, 1);
    e = s
        .review(
            e.id,
            e.revision,
            "review-0003",
            "forgot",
            T + 3 * MIN_INTERVAL,
        )
        .unwrap();
    assert_eq!(e.reviews, 0);
    assert_eq!(e.status, "review");
}
#[test]
fn quiz_waits_another_interval_and_scores_in_backend_idempotently() {
    let s = store();
    let e = at_test(&s);
    assert!(s
        .begin_quiz(e.id, e.revision, T + 4 * MIN_INTERVAL - 1)
        .is_err());
    let q = s
        .begin_quiz(e.id, e.revision, T + 4 * MIN_INTERVAL)
        .unwrap();
    assert!(!serde_json::to_string(&q).unwrap().contains("correctIndex"));
    assert!(!serde_json::to_string(&q).unwrap().contains("\"answer\""));
    let correct = q.options.iter().position(|v| v == "建筑学").unwrap();
    let r = s
        .submit_quiz(
            &q.token,
            correct,
            " ARCHITECTURE ",
            T + 4 * MIN_INTERVAL + 1,
        )
        .unwrap();
    assert!(r.passed);
    assert_eq!(r.entry.status, "mastered");
    let again = s
        .submit_quiz(&q.token, 0, "wrong", T + 4 * MIN_INTERVAL + 2)
        .unwrap();
    assert!(again.passed);
    assert_eq!(
        s.list("", "learning", 0, T + 4 * MIN_INTERVAL)
            .unwrap()
            .entries
            .len(),
        0
    );
    assert_eq!(
        s.list("", "mastered", 0, T + 4 * MIN_INTERVAL)
            .unwrap()
            .entries
            .len(),
        1
    );
}
#[test]
fn failed_quiz_requires_review_and_new_delay_and_alternates_questions() {
    let s = store();
    let e = at_test(&s);
    let q = s
        .begin_quiz(e.id, e.revision, T + 4 * MIN_INTERVAL)
        .unwrap();
    let r = s
        .submit_quiz(&q.token, 0, "wrong", T + 4 * MIN_INTERVAL + 1)
        .unwrap();
    assert!(!r.passed);
    assert_eq!(r.entry.reviews, 2);
    assert!(s
        .begin_quiz(e.id, r.entry.revision, T + 4 * MIN_INTERVAL + 2)
        .is_err());
    let e = s
        .review(
            e.id,
            r.entry.revision,
            "retry-0001",
            "remember",
            T + 5 * MIN_INTERVAL + 1,
        )
        .unwrap();
    let next = s
        .begin_quiz(e.id, e.revision, T + 6 * MIN_INTERVAL + 1)
        .unwrap();
    assert_ne!(q.cloze, next.cloze);
}
#[test]
fn abandoning_test_keeps_progress_and_stale_session_cannot_score() {
    let s = store();
    let e = at_test(&s);
    let q = s
        .begin_quiz(e.id, e.revision, T + 4 * MIN_INTERVAL)
        .unwrap();
    s.abandon_quiz(&q.token).unwrap();
    assert_eq!(s.get(e.id).unwrap().reviews, 3);
    assert!(s
        .submit_quiz(&q.token, 0, "architecture", T + 4 * MIN_INTERVAL + 1)
        .is_err());
}
#[test]
fn edit_resets_progress_and_late_generation_cannot_overwrite() {
    let s = store();
    let e = s.collect("architecture", "", "", T).unwrap();
    let running = s.generation_begin(e.id, e.revision, false).unwrap();
    let edited = s
        .save_card(e.id, running.revision, fixture_card("architecture"), T)
        .unwrap();
    assert!(s
        .generation_finish(
            e.id,
            running.content_revision,
            Ok(fixture_card("architecture")),
            "mock",
            "mock",
            T
        )
        .is_err());
    assert!(s.get(e.id).unwrap().user_edited);
    assert_eq!(edited.status, "new");
}
#[test]
fn rules_never_shorten_existing_due_and_reject_under_four_hours() {
    let s = store();
    let e = study(&s, ready(&s, "architecture"));
    s.set_rules(
        Rules {
            interval_hours: 8,
            daily_limit: 20,
        },
        T,
    )
    .unwrap();
    let postponed = s.get(e.id).unwrap().next_due_at.unwrap();
    assert_eq!(postponed, T + 8 * 3600);
    s.set_rules(Rules::default(), T).unwrap();
    assert_eq!(s.get(e.id).unwrap().next_due_at.unwrap(), postponed);
    assert!(s
        .set_rules(
            Rules {
                interval_hours: 3,
                daily_limit: 20
            },
            T
        )
        .is_err());
}
#[test]
fn clock_rollback_rejected_without_progress_mutation() {
    let s = store();
    let e = study(&s, ready(&s, "architecture"));
    let r = s
        .review(
            e.id,
            e.revision,
            "review-0001",
            "remember",
            T + MIN_INTERVAL,
        )
        .unwrap();
    assert!(s
        .review(e.id, r.revision, "review-0002", "remember", T)
        .is_err());
    assert_eq!(s.get(e.id).unwrap().reviews, 1);
}
#[test]
fn daily_cap_rolls_back_event_and_progress() {
    let s = store();
    let a = ready(&s, "architecture");
    let b = ready(&s, "curious");
    let a = study(&s, a);
    let b = s
        .review(b.id, b.revision, "study-0002", "study", T)
        .unwrap();
    s.set_rules(
        Rules {
            interval_hours: 4,
            daily_limit: 1,
        },
        T,
    )
    .unwrap();
    s.review(
        a.id,
        a.revision,
        "review-0001",
        "remember",
        T + MIN_INTERVAL,
    )
    .unwrap();
    assert!(s
        .review(
            b.id,
            b.revision,
            "review-0002",
            "remember",
            T + MIN_INTERVAL
        )
        .is_err());
    assert_eq!(s.get(b.id).unwrap().reviews, 0);
}
#[test]
fn future_schema_and_invalid_import_never_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.db");
    let c = Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA user_version=99").unwrap();
    drop(c);
    assert!(VocabularyStore::open(&path).is_err());
    let s = store();
    let e = ready(&s, "architecture");
    let mut value: serde_json::Value = serde_json::from_str(&s.export().unwrap()).unwrap();
    value["schemaVersion"] = json_value(99);
    assert!(s.import(&value.to_string(), true, T).is_err());
    assert_eq!(s.get(e.id).unwrap().word, "architecture");
    assert!(s.import("{}", false, T).is_err());
}
fn json_value(value: i64) -> serde_json::Value {
    serde_json::Value::from(value)
}
#[test]
fn json_restore_preserves_progress_context_rules_and_logs_without_secrets() {
    let s = store();
    let e = at_test(&s);
    let exported = s.export().unwrap();
    assert!(!exported.contains("apiKey"));
    let dest = store();
    assert_eq!(
        dest.import(&exported, true, T + 3 * MIN_INTERVAL).unwrap(),
        1
    );
    let restored = dest
        .list("", "test", 0, T + 3 * MIN_INTERVAL)
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(restored.reviews, e.reviews);
    assert_eq!(restored.next_due_at, e.next_due_at);
    assert_eq!(
        dest.import(&exported, true, T + 3 * MIN_INTERVAL).unwrap(),
        0
    );
    let backup: BookBackup = serde_json::from_str(&dest.export().unwrap()).unwrap();
    assert_eq!(backup.events.len(), 4);
}
#[test]
fn sqlite_wal_backup_and_reopen_preserve_exact_data() {
    let dir = tempfile::tempdir().unwrap();
    let s = VocabularyStore::open(&dir.path().join("book.db")).unwrap();
    let e = at_test(&s);
    s.backup_to(&dir.path().join("copy.db")).unwrap();
    let reopened = VocabularyStore::open(&dir.path().join("copy.db")).unwrap();
    assert_eq!(reopened.get(e.id).unwrap().next_due_at, e.next_due_at);
    assert_eq!(s.export().unwrap(), reopened.export().unwrap());
    assert!(s.backup_to(&dir.path().join("copy.db")).is_err());
}
#[test]
fn interrupted_generation_recovers_and_failure_preserves_word() {
    let s = store();
    let e = s.collect("architecture", "", "", T).unwrap();
    s.generation_begin(e.id, e.revision, false).unwrap();
    s.recover_interrupted().unwrap();
    let e = s.get(e.id).unwrap();
    assert_eq!(e.generation_state, "pending");
    let e = s.generation_begin(e.id, e.revision, false).unwrap();
    let e = s
        .generation_finish(
            e.id,
            e.content_revision,
            Err("missing key".into()),
            "mock",
            "mock",
            T,
        )
        .unwrap();
    assert!(e.card.is_none());
    assert_eq!(e.generation_state, "pending");
    assert_eq!(e.generation_error.unwrap(), "missing key");
}
#[test]
fn invalid_quiz_and_answer_leaks_are_rejected() {
    let mut c = fixture_card("architecture");
    c.quizzes[0].cloze = "She studies architecture: ____.".into();
    assert!(c.validate("architecture").is_err());
    let mut c = fixture_card("architecture");
    c.quizzes[0].options[1] = c.quizzes[0].options[0].clone();
    assert!(c.validate("architecture").is_err());
    let mut c = fixture_card("architecture");
    c.quizzes[0].answer = "unrelated".into();
    assert!(c.validate("architecture").is_err());
}
#[test]
fn repair_quiz_preserves_counts_but_invalidates_old_attempt() {
    let s = store();
    let e = at_test(&s);
    let q = s
        .begin_quiz(e.id, e.revision, T + 4 * MIN_INTERVAL)
        .unwrap();
    let current = s.get(e.id).unwrap();
    let e = s
        .replace_quizzes(
            e.id,
            current.revision,
            fixture_card("architecture").quizzes,
            T + 4 * MIN_INTERVAL,
        )
        .unwrap();
    assert_eq!(e.reviews, 3);
    assert!(s
        .submit_quiz(&q.token, 0, "architecture", T + 4 * MIN_INTERVAL + 1)
        .is_err());
}
#[test]
fn deletion_requires_confirmation_and_stale_revision_rejected() {
    let s = store();
    let e = ready(&s, "architecture");
    assert!(s.delete(e.id, e.revision, false).is_err());
    assert!(s.delete(e.id, e.revision - 1, true).is_err());
    s.delete(e.id, e.revision, true).unwrap();
    assert!(s.get(e.id).is_err());
}
#[test]
fn lemma_merge_preserves_target_and_restores_aliases() {
    let s = store();
    let dest = ready(&s, "architecture");
    let e = s
        .collect("architectures", "Modern architectures.", "", T)
        .unwrap();
    let mut card = fixture_card("architecture");
    card.word = "architectures".into();
    let e = s.save_card(e.id, e.revision, card, T).unwrap();
    let merged = s.adopt_lemma(e.id, e.revision, T).unwrap();
    assert_eq!(merged.id, dest.id);
    assert_eq!(s.collect("architectures", "", "", T).unwrap().id, dest.id);
    let exported = s.export().unwrap();
    let copy = store();
    copy.import(&exported, true, T).unwrap();
    assert_eq!(
        copy.collect("architectures", "", "", T).unwrap().word,
        "architecture"
    );
}
#[test]
fn cache_is_context_specific_and_does_not_evict_learning_data() {
    let s = store();
    let e = ready(&s, "architecture");
    s.cache("key", &fixture_card("architecture"), T).unwrap();
    assert!(s.cached("other").unwrap().is_none());
    assert!(s.cached("key").unwrap().is_some());
    assert!(s.get(e.id).unwrap().card.is_some());
}
