use chronobreak_league_data::observation::{Endpoint, decode, journal};
use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use serde_json::{Value, json};
use std::{hint::black_box, io::Cursor};

const SWIFTPLAY_JOURNAL: &[u8] = include_bytes!(
    "../../fixtures/preprod-library/library/games/1792431584/league_observations.jsonl"
);
// Match the app's bounded journal import unit.
const IMPORT_BATCHES: usize = 32;

fn live_player_list() -> Vec<u8> {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/live-endpoints.json"))
            .expect("Live Client fixture");
    let champions = [
        "Garen", "Vi", "Ahri", "Jinx", "Leona", "Sett", "XinZhao", "Lux", "Caitlyn", "Nami",
    ];
    let positions = ["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY"];
    let players: Vec<_> = champions
        .iter()
        .enumerate()
        .map(|(index, champion)| {
            let mut player = fixture["pl"][0].clone();
            let name = format!("Player{index}");
            player["riotId"] = json!(format!("{name}#TEST"));
            player["riotIdGameName"] = json!(name);
            player["summonerName"] = json!(name);
            player["championName"] = json!(champion);
            player["rawChampionName"] = json!(format!("game_character_displayname_{champion}"));
            player["team"] = json!(if index < 5 { "ORDER" } else { "CHAOS" });
            player["position"] = json!(positions[index % positions.len()]);
            player
        })
        .collect();
    serde_json::to_vec(&players).expect("ten-player response")
}

fn league_data(criterion: &mut Criterion) {
    let roster = live_player_list();
    let mut group = criterion.benchmark_group("league_data");
    group.throughput(Throughput::Bytes(roster.len() as u64));
    group.bench_function("live_player_list_10", |bencher| {
        bencher.iter(|| {
            decode::live(Endpoint::PlayerList, black_box(roster.as_slice()))
                .expect("ten-player response decodes")
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
