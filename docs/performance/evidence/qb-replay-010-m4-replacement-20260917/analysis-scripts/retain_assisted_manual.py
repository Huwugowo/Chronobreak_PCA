import datetime
import hashlib
import json
from pathlib import Path

repo=Path(__file__).resolve().parents[2]
work=repo/'build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark/manual-assisted-20260917'
out=repo/'docs/performance/evidence/qb-replay-010-m4-replacement-20260917'
def sha(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream,'sha256').hexdigest()
audit=json.loads((work/'audit-baseline.json').read_text())
for root in audit['roots'].values():
    assert not root['deleted'] and not root['added'] and not root['changed']
assert not (work/'launch.json').exists()
value=dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    status='environment-blocked',native_controls='disabled',cua_inventory={'apps':[],'browsers':[]},
    manual_actions_run=False,assisted_app_launched=False,
    prepared=True,baseline_hashes_unchanged=True,
    baseline={label:{k:v for k,v in row.items() if k!='files'} for label,row in audit['roots'].items()},
    preparation=json.loads((work/'preparation.json').read_text()),
    artifacts=[dict(path=p.relative_to(repo).as_posix(),sha256=sha(p),bytes=p.stat().st_size)
        for p in [work/'preparation.json',work/'root-a-before.json',work/'root-b-before.json',work/'audit-baseline.json',repo/'tools/replay_benchmark/qb010_manual.py']],
    verification=['py_compile passed; --help passed',
        'Earlier Windows self-process identity and Toolhelp smoke passed without suspending any process',
        'Earlier optimized-Python nine-second hold rejection passed before app access',
        'Fresh prepare and baseline audit passed; no UI/playback/export/hold pass inferred'],
    remaining=['Useful Games during owned optional work; local failure/retry; Clips select/away/back',
        'Save/delete/export/cleanup while an optional batch is actually owned',
        'Overlapping-ID A/B/A roots and fresh counts/cards',
        'Viewer return, user-observed source/export playback, H.264/AAC/duration/decode and source preservation'],
    procedure='manual-acceptance.md')
with (out/'manual-status.json').open('x',encoding='utf-8') as stream:
    json.dump(value,stream,indent=2); stream.write('\n')
print('Retained prepared, unchanged disposable roots; manual environment-blocked, no assisted UI actions.')
