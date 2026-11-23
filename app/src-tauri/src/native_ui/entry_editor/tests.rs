use super::filter::ROWS_PER_TICK;
use super::*;
use std::time::Instant;

fn synthetic_rows() -> Arc<Vec<Entry>> {
    let body = "Ein längerer synthetischer Absatz mit Umlauten. ".repeat(16);
    Arc::new(
        (0..16_384)
            .map(|id| {
                Entry::Snippet(SnippetEntry {
                    id,
                    trigger: format!("Vorlage {id}"),
                    text: if id == 16_383 {
                        format!("{body}Prüffund")
                    } else {
                        body.clone()
                    },
                    created_at: 0,
                })
            })
            .collect(),
    )
}

#[test]
fn search_coalesces_changes_and_returns_all_matching_snapshot_indices() {
    let rows = synthetic_rows();
    let mut filter = Filter::default();
    filter.request(rows.clone(), "Prüffund".into()).unwrap();
    filter.request(rows.clone(), "Vorlage 1".into()).unwrap();
    filter.request(rows.clone(), "".into()).unwrap();
    assert!(await_matches(&mut filter).into_iter().eq(0..rows.len()));
    filter.request(rows, "prÜffund".into()).unwrap();
    assert_eq!(await_matches(&mut filter), [16_383]);
    assert!(!filter.busy());
}

fn await_matches(filter: &mut Filter) -> Vec<usize> {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(matches) = filter.poll().unwrap() {
            return matches;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn list_updates_limit_work_per_tick_and_preserve_every_snapshot_index() {
    let size = 16_384;
    let mut update = ListUpdate::new(size, (0..size).step_by(2).collect());
    let mut visible = vec![usize::MAX; size];
    let mut ticks = 0;
    while !update.finished() {
        let mut changes = 0;
        update
            .tick(|change| {
                changes += 1;
                match change {
                    RowChange::Remove(index) => {
                        assert_eq!(index, visible.len() - 1);
                        visible.pop();
                    }
                    RowChange::Append(index) => visible.push(index),
                }
                Ok(())
            })
            .unwrap();
        assert!((1..=ROWS_PER_TICK).contains(&changes));
        ticks += 1;
    }
    assert!(ticks > 1);
    assert!(visible.into_iter().eq((0..size).step_by(2)));
}
