-- Disposable projection. No path or row in this database authorizes media.
CREATE TABLE IF NOT EXISTS cache_identity(root TEXT PRIMARY KEY, version INTEGER NOT NULL, catalog_token TEXT, authored_stamp TEXT, authored_status TEXT NOT NULL DEFAULT 'pending',authored_revision INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS recordings(
  media_id TEXT PRIMARY KEY, bundle TEXT NOT NULL UNIQUE, recorded_at TEXT NOT NULL,
  champion TEXT NOT NULL, mode TEXT NOT NULL, saved INTEGER NOT NULL,
  queue_id INTEGER, map_id INTEGER, outcome TEXT NOT NULL, game_id TEXT, match_group TEXT,
  summary_json TEXT NOT NULL, result_json TEXT, source_stamp TEXT, history_stamp TEXT,
  journal_header TEXT, source_identity TEXT, position_json TEXT, checkpoint_json TEXT, timeline_json TEXT,
  telemetry_status TEXT NOT NULL DEFAULT 'pending', tail_bytes INTEGER NOT NULL DEFAULT 0,
  facts_current INTEGER NOT NULL DEFAULT 0 CHECK(facts_current IN (0,1)),
  history_flags INTEGER NOT NULL DEFAULT 31 CHECK(history_flags BETWEEN 0 AND 31)
);
CREATE INDEX IF NOT EXISTS recordings_date ON recordings(recorded_at DESC,media_id,match_group) WHERE match_group IS NOT NULL;
CREATE INDEX IF NOT EXISTS recordings_match ON recordings(match_group,recorded_at DESC,media_id,saved,facts_current,history_flags) WHERE match_group IS NOT NULL;
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
-- Legacy v8 shadow of the authoritative journal. No reader uses this table.
-- Retain the schema so existing caches can be cleared without a migration.
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
-- Derived from verified, local-player event observations. This is deliberately
-- source-scoped: the match-level effective set below is rebuilt from every
-- ready checked part plus durable match overrides.
CREATE TABLE IF NOT EXISTS match_tag_observed(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  tag_id TEXT NOT NULL,
  PRIMARY KEY(media_id,tag_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS match_tag_dirty(match_group TEXT PRIMARY KEY) WITHOUT ROWID;
CREATE TRIGGER IF NOT EXISTS match_tag_recording_removed BEFORE DELETE ON recordings
WHEN OLD.match_group IS NOT NULL AND OLD.facts_current=1 AND OLD.telemetry_status='ready'
 AND EXISTS(SELECT 1 FROM match_tag_observed WHERE media_id=OLD.media_id)
BEGIN
  INSERT OR IGNORE INTO match_tag_dirty VALUES(OLD.match_group);
END;
CREATE TRIGGER IF NOT EXISTS match_tag_recording_changed
AFTER UPDATE OF match_group,facts_current,telemetry_status ON recordings
WHEN (OLD.match_group IS NOT NEW.match_group OR OLD.facts_current<>NEW.facts_current
 OR OLD.telemetry_status<>NEW.telemetry_status)
 AND EXISTS(SELECT 1 FROM match_tag_observed WHERE media_id=NEW.media_id)
BEGIN
  INSERT OR IGNORE INTO match_tag_dirty SELECT OLD.match_group
    WHERE OLD.match_group IS NOT NULL AND OLD.facts_current=1 AND OLD.telemetry_status='ready';
  INSERT OR IGNORE INTO match_tag_dirty SELECT NEW.match_group
    WHERE NEW.match_group IS NOT NULL AND NEW.facts_current=1 AND NEW.telemetry_status='ready';
END;
-- Overrides are user-authored intent, keyed by the account-scoped match group.
-- They intentionally do not reference a recording so they survive source
-- re-indexing and replacement parts.
CREATE TABLE IF NOT EXISTS match_tag_overrides(
  match_group TEXT NOT NULL, tag_id TEXT NOT NULL,
  added INTEGER NOT NULL CHECK(added IN (0,1)),
  suppressed INTEGER NOT NULL CHECK(suppressed IN (0,1)),
  PRIMARY KEY(match_group,tag_id)
);
CREATE TABLE IF NOT EXISTS match_tag_effective(
  match_group TEXT NOT NULL, tag_id TEXT NOT NULL,
  PRIMARY KEY(match_group,tag_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS match_tag_effective_tag ON match_tag_effective(tag_id,match_group);
CREATE TABLE IF NOT EXISTS tag_catalog_state(
  singleton INTEGER PRIMARY KEY CHECK(singleton=1), stamp TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tag_definition(
  id TEXT PRIMARY KEY, label TEXT NOT NULL, label_key TEXT NOT NULL UNIQUE
);
-- App-derived identity is disposable and never changes the producer's revision.
CREATE TABLE IF NOT EXISTS derived_actor_links(
  media_id TEXT NOT NULL REFERENCES recordings ON DELETE CASCADE,
  sequence INTEGER NOT NULL, revision_ordinal INTEGER NOT NULL, role TEXT NOT NULL, ordinal INTEGER NOT NULL,
  participant_id INTEGER NOT NULL, basis TEXT NOT NULL CHECK(basis='validated_roster_game_name'),
  PRIMARY KEY(media_id,sequence,revision_ordinal,role,ordinal)
);
CREATE VIEW IF NOT EXISTS effective_event_actors AS
  SELECT a.media_id,a.run_id,a.epoch,a.sequence,a.revision_ordinal,a.role,a.ordinal,
    COALESCE(d.participant_id,a.participant_id) AS participant_id,
    CASE WHEN d.participant_id IS NOT NULL THEN 'resolved' ELSE a.status END AS status
  FROM event_actors a LEFT JOIN derived_actor_links d
    ON d.media_id=a.media_id AND d.sequence=a.sequence AND d.revision_ordinal=a.revision_ordinal AND d.role=a.role AND d.ordinal=a.ordinal;
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

CREATE TABLE IF NOT EXISTS prepared_overlay_sources(media_id TEXT NOT NULL REFERENCES recordings(media_id) ON DELETE CASCADE,run_id TEXT NOT NULL,epoch INTEGER NOT NULL,revision TEXT NOT NULL,complete INTEGER NOT NULL,cursor TEXT,PRIMARY KEY(media_id,run_id,epoch));
CREATE TABLE IF NOT EXISTS prepared_overlay_samples(media_id TEXT NOT NULL REFERENCES recordings(media_id) ON DELETE CASCADE,run_id TEXT NOT NULL,epoch INTEGER NOT NULL,replay_tick INTEGER NOT NULL,observation_id INTEGER NOT NULL,data TEXT NOT NULL,PRIMARY KEY(media_id,run_id,epoch,replay_tick,observation_id));
