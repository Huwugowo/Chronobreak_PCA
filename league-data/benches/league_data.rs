use chronobreak_league_data::observation::{Endpoint, decode, journal};
use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use std::{hint::black_box, io::Cursor};

const MIDGAME_PLAYER_LIST: &[u8] = include_bytes!("fixtures/live-player-list-midgame.json");
const SWIFTPLAY_JOURNAL: &[u8] = include_bytes!(
    "../../fixtures/preprod-library/library/games/1792431584/league_observations.jsonl"
);
// Match the app's bounded journal import unit.
const IMPORT_BATCHES: usize = 32;

fn league_data(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("league_data");
    group.throughput(Throughput::Bytes(MIDGAME_PLAYER_LIST.len() as u64));
    group.bench_function("live_player_list_midgame_10", |bencher| {
        bencher.iter(|| {
            decode::live(Endpoint::PlayerList, black_box(MIDGAME_PLAYER_LIST))
                .expect("midgame roster decodes")
        });
    });

    // Start at a validated commit after the startup page, as a resumed import does.
    let mut batches = 0;
    let mut anchor = None;
    let recovery = journal::recover(Cursor::new(SWIFTPLAY_JOURNAL), |_, position| {
        batches += 1;
        if batches == IMPORT_BATCHES {
            anchor = Some(position.clone());
        }
        Ok(())
    })
    .expect("valid Swiftplay journal fixture");
    assert!(
        batches >= 2 * IMPORT_BATCHES,
        "fixture needs two full pages"
    );
    let anchor = anchor.expect("valid import anchor");

    group.sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Elements(IMPORT_BATCHES as u64));
    group.bench_function("journal_import_page_32", |bencher| {
        bencher.iter(|| {
            journal::recover_after(
                Cursor::new(black_box(SWIFTPLAY_JOURNAL)),
                &recovery.header,
                &anchor,
                IMPORT_BATCHES,
                |entries, _| {
                    black_box(entries);
                    Ok(())
                },
            )
            .expect("valid Swiftplay journal page")
        });
    });
    group.finish();
}

criterion_group!(benches, league_data);
criterion_main!(benches);
