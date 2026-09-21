"""One-shot replacement runner: immutable start/finish receipts and fail-stop."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

repo = Path(__file__).resolve().parents[2]
bench = repo/'build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark'
campaign = bench/'campaigns/replacement-20260917-r1'
mode = sys.argv[1]
assert mode in ('preflight','live')
sys.path.insert(0, str(repo/'tools/replay_benchmark'))
import analyze
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()
def write(path, value):
    with path.open('x',encoding='utf-8') as stream:
        json.dump(value,stream,indent=2); stream.write('\n'); stream.flush(); os.fsync(stream.fileno())
def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
order_path = campaign/'specs/global-order.json'
order = json.loads(order_path.read_text())
assert order['ceiling'] == len(order['launches']) == 50
for arm in ('reference','candidate'):
    folder = bench/'subjects'/arm
    subject = json.loads((folder/'subject.json').read_text())
    for record in subject['files']: assert sha(folder/record['path']) == record['sha256']
    for record in subject['source_files']: assert sha(Path(subject['source_root'])/record['path']) == record['sha256']
for launch in order['launches']:
    assert sha(Path(launch['manifest_path'])) == launch['manifest_sha256']
    assert not Path(launch['result_root']).exists()
    if mode == 'live':
        receipt = json.loads((campaign/'preflight'/f"{launch['sequence']:02}.json").read_text())
        assert receipt['exit_code'] == 0 and receipt['global_order_sha256'] == sha(order_path)
out = campaign/mode
out.mkdir(exist_ok=False)
for launch in order['launches']:
    number = launch['sequence']
    command = ['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(repo/'tools/replay_benchmark/run.ps1'),'-Manifest',launch['manifest_path']]
    if mode == 'preflight': command.append('-PreflightOnly')
    receipt = dict(sequence=number,command=command,started_utc=now(),
        manifest_sha256=launch['manifest_sha256'],global_order_sha256=sha(order_path))
    write(out/f'{number:02}.started.json',receipt)
    print(f"{mode} {number}/50 {launch['cell']}-{launch['kind']} {launch['arm']} trial {launch['trial']}",flush=True)
    log = out/f'{number:02}.log'
    with log.open('x',encoding='utf-8') as stream:
        result = subprocess.run(command,cwd=repo,stdout=stream,stderr=subprocess.STDOUT)
    receipt.update(exit_code=result.returncode,completed_utc=now(),log_sha256=sha(log))
    if result.returncode == 0 and mode == 'live':
        try:
            analyze.load_bundle(Path(launch['result_root']),f'launch-{number:02}')
            receipt['bundle_validated'] = True
        except Exception as error:
            receipt.update(exit_code=3,bundle_validated=False,error=str(error))
    write(out/f'{number:02}.json',receipt)
    if receipt['exit_code']:
        write(out/'STOPPED.json',receipt)
        print(f'STOPPED at {number}; all artifacts preserved, no next launch.',flush=True)
        print('\n'.join(log.read_text(encoding='utf-8',errors='replace').splitlines()[-8:]).encode('ascii',errors='backslashreplace').decode())
        sys.exit(receipt['exit_code'])
    if mode == 'live': time.sleep(5)
write(out/'complete.json',dict(mode=mode,count=50,completed_utc=now(),global_order_sha256=sha(order_path)))
print(f'{mode}: all 50 passed',flush=True)
