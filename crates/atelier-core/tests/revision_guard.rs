use atelier_core::{RevisionGuard, RevisionOverflow};

#[test]
fn older_result_is_rejected_after_a_new_revision_is_issued() {
    let mut guard = RevisionGuard::default();
    let older = guard.issue().expect("issue older revision");
    let newer = guard.issue().expect("issue newer revision");

    assert_eq!(guard.accept(older, "old preview"), None);
    assert_eq!(guard.accept(newer, "new preview"), Some("new preview"));
}

#[test]
fn revision_belongs_to_the_guard_that_issued_it() {
    let mut first_guard = RevisionGuard::default();
    let mut second_guard = RevisionGuard::default();
    let first_revision = first_guard.issue().expect("issue first revision");
    let second_revision = second_guard.issue().expect("issue second revision");

    assert_eq!(first_guard.accept(second_revision, "wrong job"), None);
    assert_eq!(second_guard.accept(first_revision, "wrong job"), None);
}

#[test]
fn revision_counter_never_wraps_to_an_older_value() {
    let mut guard = RevisionGuard::from_counter_for_test(u64::MAX);

    assert_eq!(guard.issue(), Err(RevisionOverflow));
}
