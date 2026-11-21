use super::{
    ClipGridFilter, ClipGridOutcome, ClipGridQuery, GRID_PAGE_SIZE, SCHEMA, grid_page_sql,
};
use crate::league_index::query::HistoryQueue;
use crate::tags::TagId;
use rusqlite::{Connection, StatementStatus, ToSql, params, params_from_iter};
use std::time::Instant;

const RANKED: &str = "custom:11111111-2222-4333-8444-555555555555";
const COMEBACK: &str = "custom:11111111-2222-4333-8444-555555555556";

fn populate(db: &Connection, count: i64) {
    db.execute_batch(SCHEMA).unwrap();
    // Seed the same projection and cache schema as the application, without touching media.
    db.execute(
        "WITH RECURSIVE clips(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM clips WHERE n<?1)
         INSERT INTO clip_projection
         SELECT printf('grid-%08d',n),'fixture',printf('match-%08d',n/3),printf('match-%08d',n/3),
                unixepoch(CASE WHEN n<=?1/5 THEN '2026-10-01' ELSE '2026-11-01' END),
                n*160, n*900,
                CASE WHEN n%21=0 THEN 'ahri' ELSE 'lux' END,
                CASE WHEN n<=?1/5 THEN '2026-10-01' ELSE '2026-11-01' END,
                CASE WHEN n%3=0 THEN '420' ELSE '400' END,
                CASE WHEN n%7=0 THEN 'win' ELSE 'loss' END,
                lower(printf('Solo queue %s | %s | %s',n,
                    CASE WHEN n%19=0 THEN 'Tower dive' ELSE 'Team fight' END,
                    CASE WHEN n%43=0 THEN 'Pentakill' ELSE 'Double Kill' END)),
                json_object('exported_at', n/4, 'game', json_object('mode', 'CLASSIC',
                    'champion','Lux','account','Fixture#EUW','played_at','2026-11-01T12:00:00Z'),
                    'id',printf('grid-%08d',n),'source_timestamp','1-2','duration','240000000',
                    'range',json_object('start_frame',n*900,'end_frame_exclusive',n*900+300),
                    'preset','horizontal','version',3)
         FROM clips",
        params![count],
    )
    .unwrap();
    db.execute("INSERT INTO clip_tag_projection
         SELECT id,CASE WHEN (frame/900)%251=0 THEN ?1 ELSE ?2 END,
              CASE WHEN (frame/900)%251=0 THEN 'Late comeback' ELSE 'Ranked' END,
              CASE WHEN (frame/900)%251=0 THEN 'late comeback' ELSE 'ranked' END FROM clip_projection",
         params![COMEBACK,RANKED]).unwrap();
    db.execute_batch(
        "CREATE TABLE tag_definition(id TEXT PRIMARY KEY,label TEXT,label_key TEXT UNIQUE);
         INSERT INTO clip_grid_tag_projection
         SELECT tag.tag_id,clip.created,clip.id FROM clip_tag_projection tag
             JOIN clip_projection clip ON clip.id=tag.clip_id;
         INSERT INTO clip_grid_tag_projection
         SELECT 'builtin:double_kill',created,id FROM clip_projection;
         INSERT INTO clip_grid_tag_projection
         SELECT keys.value,created,clip_projection.id FROM clip_projection,
             json_each(json_array('builtin:triple_kill','builtin:quadra_kill','builtin:pentakill')) keys
         WHERE (frame/900)%43=0;
         ANALYZE;",
    )
    .unwrap();
}

fn measure(
    db: &Connection,
    name: &str,
    query: ClipGridQuery,
    after: Option<i64>,
    expected: &[i64],
) {
    let after_id = after.map(|n| format!("grid-{n:08}"));
    let created = after.map(|n| n / 4);
    let filter = ClipGridFilter::new(&query).unwrap();
    let limit = (GRID_PAGE_SIZE + 1) as i64;
    let arguments = || {
        filter.args().chain([&limit as &dyn ToSql]).chain(
            after
                .map(|_| [&created as &dyn ToSql, &after_id as &dyn ToSql])
                .into_iter()
                .flatten(),
        )
    };
    let sql = grid_page_sql(&filter, after.is_some());
    let plan = db
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap()
        .query_map(params_from_iter(arguments()), |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut statement = db.prepare(&sql).unwrap();
    let cases = [
        StatementStatus::VmStep,
        StatementStatus::FullscanStep,
        StatementStatus::Sort,
    ];
    let mut samples = Vec::new();
    let run = |statement: &mut rusqlite::Statement<'_>| {
        let start = Instant::now();
        let mut rows = statement.query(params_from_iter(arguments())).unwrap();
        let mut ids = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            ids.push(row.get::<_, String>(0).unwrap());
            std::hint::black_box(row.get_ref(2).unwrap());
        }
        (ids, start.elapsed())
    };
    let (rows, _) = run(&mut statement);
    assert_eq!(
        rows,
        expected
            .iter()
            .take(GRID_PAGE_SIZE + 1)
            .map(|n| format!("grid-{n:08}"))
            .collect::<Vec<_>>(),
        "{name}"
    );
    for _ in 0..15 {
        samples.push(run(&mut statement).1.as_micros());
    }
    samples.sort_unstable();
    for counter in cases {
        statement.reset_status(counter);
    }
    run(&mut statement);
    let [vm_steps, scan_steps, sorts] = cases.map(|counter| statement.get_status(counter));
    assert_eq!(sorts, 0, "{name}: {plan:?}");
    assert!(
        !plan.iter().any(|line| line.contains("TEMP B-TREE")),
        "{name}: {plan:?}"
    );
    if !query.tags.is_empty() {
        assert!(
            plan.iter()
                .any(|line| line.contains("SEARCH anchor") && line.contains("tag_id=?")),
            "{name}: {plan:?}"
        );
        assert_eq!(scan_steps, 0, "{name}: {plan:?}");
    }
    eprintln!(
        "{name:24} median {:>6} µs, vm={vm_steps:>8} fullscan={scan_steps:>6} sorts={sorts} | {}",
        samples[samples.len() / 2],
        plan.join("; ")
    );
}

#[test]
#[ignore = "targeted warmed SQLite grid measurement; run in release mode"]
fn representative_clip_grid_queries() {
    for count in [1_000, 10_000, 50_000] {
        let db = Connection::open_in_memory().unwrap();
        populate(&db, count);
        eprintln!("{count} projected clips");
        let desc = (1..=count).rev();
        let all = desc.clone().collect::<Vec<_>>();
        measure(&db, "Newest", ClipGridQuery::default(), None, &all);
        let deep = count / 4;
        let after = desc.clone().filter(|&n| n < deep).collect::<Vec<_>>();
        measure(
            &db,
            "Deep cursor",
            ClipGridQuery::default(),
            Some(deep),
            &after,
        );
        let earlier = desc
            .clone()
            .filter(|n| *n <= count / 5 && n % 21 == 0)
            .collect::<Vec<_>>();
        measure(
            &db,
            "Older match filters",
            ClipGridQuery {
                champions: vec!["Ahri".into()],
                played_from: super::date_value(Some("2026-10-01T00:00:00Z")),
                played_before: super::date_value(Some("2026-10-02T00:00:00Z")),
                queue: Some(HistoryQueue::RankedSoloDuo),
                outcome: Some(ClipGridOutcome::Win),
                ..Default::default()
            },
            None,
            &earlier,
        );
        for (name, search, modulo) in [
            ("Title substring", "wer div", 19),
            ("Tag substring", "te comeb", 251),
        ] {
            let matches = desc.clone().filter(|n| n % modulo == 0).collect::<Vec<_>>();
            measure(
                &db,
                name,
                ClipGridQuery {
                    search: Some(search.into()),
                    ..Default::default()
                },
                None,
                &matches,
            );
        }
        measure(
            &db,
            "Exact predefined tag",
            ClipGridQuery {
                tags: vec![TagId::parse("builtin:pentakill").unwrap()],
                ..Default::default()
            },
            None,
            &desc.clone().filter(|n| n % 43 == 0).collect::<Vec<_>>(),
        );
        measure(
            &db,
            "Tier and custom seek",
            ClipGridQuery {
                tags: vec![
                    TagId::parse("builtin:quadra_kill").unwrap(),
                    TagId::parse(RANKED).unwrap(),
                ],
                ..Default::default()
            },
            Some(deep),
            &desc
                .clone()
                .filter(|&n| n < deep && n % 43 == 0 && n % 251 != 0)
                .collect::<Vec<_>>(),
        );
        measure(
            &db,
            "Missing substring",
            ClipGridQuery {
                search: Some("nomatch".into()),
                ..Default::default()
            },
            None,
            &[],
        );
    }
}
