-- Disposable projection. No path or row in this database authorizes media.
CREATE TABLE IF NOT EXISTS cache_identity(root TEXT PRIMARY KEY, version INTEGER NOT NULL, catalog_token TEXT, authored_stamp TEXT, authored_status TEXT NOT NULL DEFAULT 'pending',authored_revision INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS recordings(
  media_id TEXT PRIMARY KEY, bundle TEXT NOT NULL UNIQUE, recorded_at TEXT NOT NULL,
  champion TEXT NOT NULL, mode TEXT NOT NULL, saved INTEGER NOT NULL,
  queue_id INTEGER, map_id INTEGER, outcome TEXT NOT NULL, game_id TEXT,
  summary_json TEXT NOT NULL, result_json TEXT, source_stamp TEXT,
  journal_header TEXT, source_identity TEXT, position_json TEXT, checkpoint_json TEXT, timeline_json TEXT,
  telemetry_status TEXT NOT NULL DEFAULT 'pending', tail_bytes INTEGER NOT NULL DEFAULT 0,
  facts_current INTEGER NOT NULL DEFAULT 0 CHECK(facts_current IN (0,1)),
  history_flags INTEGER NOT NULL DEFAULT 31 CHECK(history_flags BETWEEN 0 AND 31)
);
-- At most 64 rows; page reads never aggregate historical observations.
CREATE TABLE IF NOT EXISTS history_counts(mask INTEGER PRIMARY KEY CHECK(mask BETWEEN 0 AND 63), count INTEGER NOT NULL CHECK(count>=0));
CREATE TRIGGER IF NOT EXISTS history_count_insert AFTER INSERT ON recordings BEGIN
  INSERT INTO history_counts VALUES (NEW.history_flags | CASE WHEN NEW.facts_current=0 THEN 32 ELSE 0 END,1)
    ON CONFLICT(mask) DO UPDATE SET count=count+1;
END;
CREATE TRIGGER IF NOT EXISTS history_count_delete AFTER DELETE ON recordings BEGIN
  UPDATE history_counts SET count=count-1 WHERE mask=(OLD.history_flags | CASE WHEN OLD.facts_current=0 THEN 32 ELSE 0 END);
END;
CREATE TRIGGER IF NOT EXISTS history_count_update AFTER UPDATE OF history_flags,facts_current ON recordings BEGIN
  UPDATE history_counts SET count=count-1 WHERE mask=(OLD.history_flags | CASE WHEN OLD.facts_current=0 THEN 32 ELSE 0 END);
  INSERT INTO history_counts VALUES (NEW.history_flags | CASE WHEN NEW.facts_current=0 THEN 32 ELSE 0 END,1)
    ON CONFLICT(mask) DO UPDATE SET count=count+1;
END;
CREATE INDEX IF NOT EXISTS recordings_date ON recordings(recorded_at DESC, media_id);
CREATE INDEX IF NOT EXISTS recordings_queue_outcome_date ON recordings(queue_id, outcome, recorded_at DESC, media_id);
CREATE TABLE IF NOT EXISTS commits(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  sequence INTEGER NOT NULL, position_json TEXT NOT NULL, epoch INTEGER NOT NULL,
  PRIMARY KEY(media_id,sequence)
);
CREATE TABLE IF NOT EXISTS scopes(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, local_participant INTEGER,
  PRIMARY KEY(media_id,run_id,epoch)
);
-- Whole-source event coverage for replay scrubbing, refreshed atomically when
-- a projection becomes ready. Range-specific absence/gap queries use source rows.
CREATE TABLE IF NOT EXISTS source_coverage(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL,
  endpoint TEXT NOT NULL, gap_attempts INTEGER NOT NULL, unresolved_actors INTEGER NOT NULL,
  unmapped_observations INTEGER NOT NULL, successful_observations INTEGER NOT NULL,
  observed_from INTEGER, observed_through INTEGER,
  PRIMARY KEY(media_id,run_id,epoch,endpoint)
);
CREATE TABLE IF NOT EXISTS evidence(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, observation_id INTEGER NOT NULL,
  endpoint TEXT NOT NULL, start_us INTEGER NOT NULL, end_us INTEGER NOT NULL,
  game_tick INTEGER, game_stats_id INTEGER, calibration_id INTEGER, data TEXT NOT NULL,
  observed INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(media_id,run_id,epoch,observation_id)
);
CREATE INDEX IF NOT EXISTS evidence_endpoint_time ON evidence(media_id,run_id,epoch,endpoint,end_us);
CREATE TABLE IF NOT EXISTS records(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  sequence INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(media_id,sequence)
);
CREATE TABLE IF NOT EXISTS calibrations(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, observation_id INTEGER NOT NULL,
  data TEXT NOT NULL, PRIMARY KEY(media_id,run_id,epoch,observation_id)
);
CREATE TABLE IF NOT EXISTS participants(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, participant_id INTEGER NOT NULL,
  riot_id TEXT, champion TEXT, team TEXT, PRIMARY KEY(media_id,run_id,epoch,participant_id)
);
CREATE INDEX IF NOT EXISTS participants_identity ON participants(riot_id,champion,media_id);
CREATE TABLE IF NOT EXISTS roster_payloads(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, payload_id INTEGER NOT NULL,
  data TEXT NOT NULL, PRIMARY KEY(media_id,run_id,epoch,payload_id)
);
CREATE TABLE IF NOT EXISTS rosters(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, observation_id INTEGER NOT NULL,
  game_tick INTEGER, replay_tick INTEGER, PRIMARY KEY(media_id,run_id,epoch,observation_id)
);
CREATE INDEX IF NOT EXISTS rosters_at_time ON rosters(media_id,run_id,epoch,game_tick DESC,observation_id DESC);
CREATE INDEX IF NOT EXISTS rosters_at_replay ON rosters(media_id,run_id,epoch,replay_tick DESC,observation_id DESC);
CREATE TABLE IF NOT EXISTS roster_members(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, observation_id INTEGER NOT NULL,
  ordinal INTEGER NOT NULL, participant_id INTEGER, payload_id INTEGER NOT NULL, game_tick INTEGER,
  PRIMARY KEY(media_id,run_id,epoch,observation_id,ordinal)
);
CREATE INDEX IF NOT EXISTS participant_at_time ON roster_members(media_id,run_id,epoch,participant_id,game_tick DESC,observation_id DESC);
CREATE TABLE IF NOT EXISTS event_revisions(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, sequence INTEGER NOT NULL, ordinal INTEGER NOT NULL,
  source_id INTEGER, fingerprint BLOB NOT NULL, response_fingerprint BLOB NOT NULL,
  observation_id INTEGER NOT NULL, game_tick INTEGER, kind TEXT, stolen INTEGER, kill_streak INTEGER,
  ingestion TEXT NOT NULL, data TEXT NOT NULL,
  PRIMARY KEY(media_id,sequence,ordinal)
);
CREATE INDEX IF NOT EXISTS event_revision_latest ON event_revisions(media_id,run_id,epoch,source_id,sequence DESC,ordinal DESC);
CREATE TABLE IF NOT EXISTS events(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, source_id INTEGER NOT NULL,
  sequence INTEGER NOT NULL, ordinal INTEGER NOT NULL, game_tick INTEGER, kind TEXT,
  stolen INTEGER, kill_streak INTEGER, ingestion TEXT NOT NULL, replay_tick INTEGER,
  PRIMARY KEY(media_id,run_id,epoch,source_id)
);
CREATE INDEX IF NOT EXISTS events_window ON events(media_id,run_id,epoch,game_tick,source_id);
CREATE INDEX IF NOT EXISTS events_replay_window ON events(media_id,run_id,epoch,replay_tick,source_id);
CREATE INDEX IF NOT EXISTS events_kind ON events(kind,media_id,game_tick);
CREATE TABLE IF NOT EXISTS event_actors(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, sequence INTEGER NOT NULL, revision_ordinal INTEGER NOT NULL,
  role TEXT NOT NULL, ordinal INTEGER NOT NULL, participant_id INTEGER, status TEXT NOT NULL,
  PRIMARY KEY(media_id,sequence,revision_ordinal,role,ordinal)
);
CREATE INDEX IF NOT EXISTS event_actor_predicate ON event_actors(media_id,run_id,epoch,participant_id,role,sequence,revision_ordinal);
CREATE TABLE IF NOT EXISTS gaps(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  run_id TEXT NOT NULL, epoch INTEGER NOT NULL, sequence INTEGER NOT NULL,
  endpoint TEXT NOT NULL, first_us INTEGER NOT NULL, last_us INTEGER NOT NULL,
  game_tick INTEGER, attempts INTEGER NOT NULL, reason TEXT NOT NULL, data TEXT NOT NULL,
  PRIMARY KEY(media_id,sequence)
);
CREATE INDEX IF NOT EXISTS gaps_interval ON gaps(media_id,run_id,epoch,endpoint,game_tick);
CREATE TABLE IF NOT EXISTS final_players(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  player_key TEXT NOT NULL, riot_id TEXT, puuid TEXT, team_id INTEGER, champion_id INTEGER,
  is_local INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(media_id,player_key)
);
CREATE INDEX IF NOT EXISTS final_player_identity ON final_players(riot_id,champion_id,media_id);
CREATE TABLE IF NOT EXISTS final_stats(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  owner_key TEXT NOT NULL, stat TEXT NOT NULL, value INTEGER, issue TEXT, aliases TEXT NOT NULL,
  PRIMARY KEY(media_id,owner_key,stat)
);
-- Authored records own their references even when a recording has been deleted.
CREATE TABLE IF NOT EXISTS authored_records(record_id TEXT PRIMARY KEY,kind TEXT NOT NULL,created_at TEXT NOT NULL,data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS authored_inputs(record_id TEXT NOT NULL REFERENCES authored_records ON DELETE CASCADE,ordinal INTEGER NOT NULL,media_id TEXT NOT NULL,kind TEXT NOT NULL,data TEXT NOT NULL,PRIMARY KEY(record_id,ordinal));
CREATE INDEX IF NOT EXISTS authored_input_media ON authored_inputs(media_id,record_id);
CREATE TABLE IF NOT EXISTS authored_ranges(record_id TEXT NOT NULL REFERENCES authored_records ON DELETE CASCADE,ordinal INTEGER NOT NULL,media_id TEXT NOT NULL,start_frame INTEGER NOT NULL,end_frame_exclusive INTEGER NOT NULL,PRIMARY KEY(record_id,ordinal));
