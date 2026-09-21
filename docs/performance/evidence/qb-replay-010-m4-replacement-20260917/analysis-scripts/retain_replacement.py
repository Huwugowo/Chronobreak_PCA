"""Retain obtained replacement evidence, never pool/repair the prior campaign."""
import datetime
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys

repo=Path(__file__).resolve().parents[2]
bench=repo/'build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark'
campaign=bench/'campaigns/replacement-20260917-r1'
out=repo/'docs/performance/evidence/qb-replay-010-m4-replacement-20260917'
sys.path.insert(0,str(repo/'tools/replay_benchmark'))
import analyze
import opening_attribution
def sha(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream,'sha256').hexdigest()
def save(name,value):
    with (out/name).open('x',encoding='utf-8') as stream: json.dump(value,stream,indent=2,sort_keys=True); stream.write('\n')
order=json.loads((campaign/'specs/global-order.json').read_text())
valid=[]; records=[]
for launch in order['launches']:
    number=launch['sequence']; root=Path(launch['result_root'])
    record={k:launch[k] for k in ('sequence','pair','cell','kind','arm','trial','run_id','manifest_sha256')}
    receipt=campaign/'live'/f'{number:02}.json'
    started=campaign/'live'/f'{number:02}.started.json'
    assert sha(Path(launch['manifest_path'])) == launch['manifest_sha256']
    if receipt.exists():
        result=json.loads(receipt.read_text())
        if result['exit_code'] == 0:
            analyze.load_bundle(root,f'launch-{number:02}')
            record['status']='valid'; valid.append(launch)
        else: record.update(status='invalid',exit_code=result['exit_code'])
    elif started.exists(): record['status']='invalid_incomplete'
    else:
        assert not root.exists()
        record['status']='not_launched'
    record['artifacts']=[dict(path=p.relative_to(bench).as_posix(),bytes=p.stat().st_size,sha256=sha(p))
        for p in sorted(root.rglob('*')) if p.is_file()] if root.exists() else []
    records.append(record)
reports={}
for arm in ('reference','candidate'):
    paths=[Path(x['result_root']) for x in valid if x['arm']==arm]
    assert paths
    reports[arm]=analyze.analyze_many(paths)
    analyze.write_reports(reports[arm],out/arm)
save('comparison.json',analyze.compare_reports(reports['reference'],reports['candidate']))
opening=[]
for launch in valid:
    result=opening_attribution.extract(Path(launch['result_root']),f"launch-{launch['sequence']:02}")
    cycles=result['cycles']
    entry={k:launch[k] for k in ('sequence','pair','cell','kind','arm','trial')}
    entry.update(initial_mount=cycles[0],warm_remounts=cycles[1:],same_clock_rule='frontend only; no native/server join')
    entry['warm_remount_process_medians']={k:statistics.median(x[k] for x in cycles[1:])
        for k in cycles[0] if k.endswith('_ms')} if len(cycles)>1 else None
    opening.append(entry)
save('opening.json',opening)
checks=[]
for path in sorted((campaign/'matrix-plans').glob('*/matrix-plan.json')):
    result=subprocess.run([sys.executable,'tools/replay_benchmark/matrix.py','verify','--plan',str(path),'--require-results'],cwd=repo,capture_output=True,text=True)
    checks.append(dict(plan=path.parent.name,exit_code=result.returncode,output=(result.stdout+result.stderr).replace(str(repo),'<repo>')))
save('matrix-verification.json',checks)
subjects=[]
for arm in ('reference','candidate','manual'):
    folder=bench/'subjects'/arm; subject=json.loads((folder/'subject.json').read_text())
    for x in subject['files']: assert sha(folder/x['path'])==x['sha256']
    for x in subject['source_files']: assert sha(Path(subject['source_root'])/x['path'])==x['sha256'],x['path']
    subjects.append(dict(arm=arm,source_identity_sha256=subject['source_identity_sha256'],executable_sha256=sha(folder/'league-replay-app.exe'),subject_receipt_sha256=sha(folder/'subject.json')))
preservation=[]
for cell in ('s','n','e'):
    receipt=json.loads((bench/'manifests'/f'prepared-{cell}.copy-receipt.json').read_text())
    for x in receipt['copied_files']:
        path=bench/f'library-{cell}'/x['relative_path']
        assert sha(path)==x['sha256'] and path.stat().st_size==x['bytes']
    preservation.append(dict(cell=cell,files=len(receipt['copied_files']),bytes=sum(x['bytes'] for x in receipt['copied_files']),all_sha256_match=True))
prior=json.loads((repo/'docs/performance/evidence/qb-replay-010-m4-20260917/provenance.json').read_text())
for launch in prior['launches']:
    for x in launch['artifacts']: assert sha(bench/x['path'])==x['sha256']
save('source-preservation.json',dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),benchmark=preservation,prior_raw_artifacts_unchanged=True))
receipts=[]
for path in sorted((repo/'build/qb010-m4/checks').glob('r2-*.json')):
    value=json.loads(path.read_text()); value['receipt']=path.name
    assert sha(path.with_suffix('.log'))==value['log_sha256']
    receipts.append(json.loads(json.dumps(value).replace(str(repo).replace('\\','\\\\'),'<repo>')))
save('verification.json',receipts)
save('provenance.json',dict(date='2026-09-17',launch_ceiling=50,prior_attempts=15,
    attempted=sum(x['status']!='not_launched' for x in records),valid=len(valid),
    invalid=sum(x['status'].startswith('invalid') for x in records),
    frozen_order_sha256=sha(campaign/'specs/global-order.json'),subjects=subjects,
    helper_sha256=sha(Path(__file__)),launches=records,
    disposition='Fresh replacement only; prior 15 attempts retained separately, never pooled. See disposition.md.'))
print(f'Retained {len(valid)} valid replacement runs; {sum(x["exit_code"]==0 for x in checks)}/10 matrices verified.')
