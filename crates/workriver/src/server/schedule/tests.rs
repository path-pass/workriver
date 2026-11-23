// The schedule's list, without a service (the timers are tested with one: server/tests/schedule.rs).
use std::time::Duration;

use super::*;

fn nothing(_: &App) -> Report {
    Report { ok: true, words: String::new(), detail: serde_json::Value::Null }
}

const ONE: Job = Job { name: "one", due: Due::Every { first: Duration::from_secs(1), every: Duration::from_secs(1) }, run: nothing };

#[test]
#[should_panic(expected = "two scheduled jobs are named one")]
fn two_jobs_of_one_name_are_refused_at_start() {
    Schedule::of(vec![ONE, Job { name: "two", ..ONE }, ONE]);
}

#[test]
fn no_jobs_show_nothing_and_a_job_shows_before_it_runs() {
    assert!(Schedule::of(vec![]).last().is_empty());
    let shown = serde_json::to_value(Schedule::of(vec![ONE]).last()).unwrap();
    assert_eq!(shown, serde_json::json!([{ "name": "one", "next": null, "running": false, "last": null }]));
}
