-- Keep one union of coverage flags per match and a bounded histogram of those
-- unions. Bit 32 represents stale facts, independently of the imported flags.
CREATE VIEW history_coverage_group_masks AS
SELECT match_group,
    MAX(facts_current=0)*32 | MAX(history_flags & 16) | MAX(history_flags & 8)
    | MAX(history_flags & 4) | MAX(history_flags & 2) | MAX(history_flags & 1) AS flags
FROM recordings WHERE match_group IS NOT NULL GROUP BY match_group;

CREATE TABLE history_coverage_groups(match_group TEXT PRIMARY KEY, flags INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE history_coverage_counts(
    flags INTEGER PRIMARY KEY CHECK(flags BETWEEN 0 AND 63),
    matches INTEGER NOT NULL CHECK(matches >= 0)
);

INSERT INTO history_coverage_groups SELECT * FROM history_coverage_group_masks;
INSERT INTO history_coverage_counts SELECT flags, COUNT(*) FROM history_coverage_groups GROUP BY flags;

-- Recompute only the affected group, using recordings_match. The adjustment
-- participates in the recording write's transaction, including rollback.
CREATE VIEW history_coverage_refresh AS SELECT match_group FROM history_coverage_groups;
CREATE TRIGGER history_coverage_refresh_group INSTEAD OF INSERT ON history_coverage_refresh
BEGIN
    UPDATE history_coverage_counts SET matches=matches-1 WHERE flags=(
        SELECT flags FROM history_coverage_groups WHERE match_group=NEW.match_group
    );
    DELETE FROM history_coverage_groups WHERE match_group=NEW.match_group;
    INSERT INTO history_coverage_groups
        SELECT match_group, flags FROM history_coverage_group_masks WHERE match_group=NEW.match_group;
    INSERT INTO history_coverage_counts
        SELECT flags, 1 FROM history_coverage_groups WHERE match_group=NEW.match_group
        ON CONFLICT(flags) DO UPDATE SET matches=matches+1;
END;

CREATE TRIGGER history_coverage_recording_insert AFTER INSERT ON recordings
WHEN NEW.match_group IS NOT NULL
BEGIN
    INSERT INTO history_coverage_refresh VALUES (NEW.match_group);
END;

CREATE TRIGGER history_coverage_recording_delete AFTER DELETE ON recordings
WHEN OLD.match_group IS NOT NULL
BEGIN
    INSERT INTO history_coverage_refresh VALUES (OLD.match_group);
END;

CREATE TRIGGER history_coverage_recording_update AFTER UPDATE OF match_group, facts_current, history_flags ON recordings
WHEN OLD.match_group IS NOT NEW.match_group OR OLD.facts_current!=NEW.facts_current OR OLD.history_flags!=NEW.history_flags
BEGIN
    INSERT INTO history_coverage_refresh SELECT OLD.match_group
        WHERE OLD.match_group IS NOT NULL AND OLD.match_group IS NOT NEW.match_group;
    INSERT INTO history_coverage_refresh SELECT NEW.match_group WHERE NEW.match_group IS NOT NULL;
END;
