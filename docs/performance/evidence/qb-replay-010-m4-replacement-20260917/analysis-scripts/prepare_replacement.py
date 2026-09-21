"""Prepare the explicitly requested fresh M4 replacement; never launch or repair old runs."""
import copy
import datetime
import hashlib
import importlib.util
import json
from pathlib import Path
import random
import sys

repo = Path(__file__).resolve().parents[2]
bench = repo / 'build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark'
campaign = bench / 'campaigns/replacement-20260917-r1'
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        assert json.loads(path.read_text()) == value, path
        return
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2); stream.write('\n')

assert not (campaign/'specs/global-order.json').exists()
assert not list(campaign.glob('matrix-plans/*/matrix-plan.json'))
assert not list(campaign.glob('results/*/*'))
prior = json.loads((repo/'docs/performance/evidence/qb-replay-010-m4-20260917/provenance.json').read_text())
assert prior['attempted'] == 15 and prior['valid'] == 14 and prior['invalid'] == 1
for launch in prior['launches']:
    for record in launch['artifacts']:
        assert sha(bench/record['path']) == record['sha256'], record['path']
for arm in ('reference', 'candidate', 'manual'):
    folder = bench/'subjects'/arm
    subject = json.loads((folder/'subject.json').read_text())
    for record in subject['files']:
        assert sha(folder/record['path']) == record['sha256'], record['path']
    for record in subject['source_files']:
        assert sha(Path(subject['source_root'])/record['path']) == record['sha256'], record['path']
files = 0
for cell in ('s','n','e'):
    receipt = json.loads((bench/'manifests'/f'prepared-{cell}.copy-receipt.json').read_text())
    for item in receipt['copied_files']:
        path = bench/f'library-{cell}'/item['relative_path']
        assert path.stat().st_size == item['bytes'] and sha(path) == item['sha256']
        files += 1
assert files == 409

spec = importlib.util.spec_from_file_location('replacement_matrix', repo/'tools/replay_benchmark/matrix.py')
matrix = importlib.util.module_from_spec(spec); sys.modules[spec.name] = matrix; spec.loader.exec_module(matrix)
plans = {}
(campaign/'matrix-plans').mkdir(parents=True, exist_ok=True)
for arm in ('reference','candidate'):
    (campaign/'results'/arm).mkdir(parents=True, exist_ok=True)
    for cell in ('s','n','e'):
        original = bench/'manifests'/f'prepared-{cell}-{arm}.json'
        value = json.loads(original.read_text())
        # Keep exact frozen fixture/config/binary paths and hashes. Fresh mutable roots only.
        run_id = f'm4r2-{cell}-{arm}-prepared'
        appdata = campaign/f'appdata-{cell}-{arm}'
        (appdata/'ddragon').mkdir(parents=True, exist_ok=True)
        scratch = campaign/f'scratch-{cell}-{arm}'; scratch.mkdir(exist_ok=True)
        value.update(run_id=run_id, app_data_root=str(appdata), scratch_root=str(scratch),
                     result_root=str(campaign/'results'/arm/run_id))
        value['ddragon']['cache_root'] = str(appdata/'ddragon')
        template = campaign/'manifests'/f'prepared-{cell}-{arm}.json'
        write(template, value)
        for kind in (('cold',) if cell == 's' else ('cold','warm')):
            matrix_id = f'm4r2-{cell}-{kind}-{arm}'
            scenario = dict(id=f'qb010-library-v2-{cell}-{kind}', kind=f'{kind}_open',
                fixture_ids=['cell-s' if cell=='s' else 'scale-00'], warmup_seconds=5, seed=20260915)
            if kind == 'warm': scenario['iterations'] = 6
            specification = dict(schema_version=1, matrix_id=matrix_id, seed=20260915,
                cooldown_seconds=5, timeout_seconds=180, plan_root=str(campaign/'matrix-plans'/matrix_id),
                arms=[dict(id=f'{cell}-{kind}', observer_profile='full', repetitions=5, scenario=scenario)],
                order=[dict(arm_id=f'{cell}-{kind}', trial_id=f'{kind}-{i:02}') for i in range(1,6)])
            spec_path = campaign/'specs'/f'{matrix_id}.json'; write(spec_path, specification)
            plan_path = matrix.plan_matrix(template, spec_path)
            matrix.verify_matrix(plan_path, require_results=False)
            plans[cell,kind,arm] = json.loads(plan_path.read_text())
rng = random.Random(20260915)
pairs = []
for trial in range(1,6):
    cells = [('s','cold'),('n','cold'),('e','cold'),('n','warm'),('e','warm')]
    rng.shuffle(cells); pairs.extend((cell,kind,trial) for cell,kind in cells)
launches = []
for pair,(cell,kind,trial) in enumerate(pairs):
    for arm in (('reference','candidate') if pair%2==0 else ('candidate','reference')):
        item = copy.deepcopy(plans[cell,kind,arm]['launches'][trial-1])
        item.update(sequence=len(launches)+1, pair=pair+1, cell=cell, kind=kind, trial=trial, arm=arm)
        launches.append(item)
assert len(launches) == 50
write(campaign/'specs/global-order.json', dict(seed=20260915, cooldown_seconds=5, ceiling=50,
    launches=launches, prior_attempts=15, maximum_total_attempts=65,
    invalid_policy='Stop first invalid; preserve all evidence; no auto-resume, selective top-up, or extra launch.',
    cache_policy='Independent processes; unflushed OS cache; post hashes outside timing; fresh separate fixed arm appdata.',
    paired_input_policy='Exact frozen subjects, library/config bytes, fixtures and measurement contract from first M4 campaign.'))
write(campaign/'preparation.json', dict(prepared_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    prior_artifacts_verified=True, frozen_subjects_and_sources_verified=True, fixture_files_verified=files,
    plans=10, launches=50, config_bytes_unchanged=True, source_media_not_redecoded=True))
print('Replacement prepared: 50 launches, 10 verified plans, 409 fixture hashes, unchanged subjects/config; prior artifacts verified.')
