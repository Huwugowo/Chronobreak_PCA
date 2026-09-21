"""User-assisted M4 acceptance on disposable fixtures; never drives the UI.

Run --help. Evidence is local beneath the existing sentinel. This helper is not
a benchmark, and its optional child hold must never run during a benchmark.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as w
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

REPO = Path(__file__).resolve().parents[2]
BENCH = REPO / 'build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark'
WORK = BENCH / 'manual-assisted-20260917'
SUBJECT = BENCH / 'subjects/manual'
APP = SUBJECT / 'league-replay-app.exe'
PROBE = SUBJECT / 'resources/media-runtime/bin/ffprobe.exe'
FFMPEG = SUBJECT / 'resources/media-runtime/bin/ffmpeg.exe'


def require(condition, message='Manual acceptance safety check failed'):
    if not condition:
        raise RuntimeError(message)


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def save(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2); stream.write('\n')
        stream.flush(); os.fsync(stream.fileno())


def inventory(root):
    require((root / '.m4-disposable-library').is_file(), 'Disposable sentinel missing')
    require(root.resolve().is_relative_to(WORK.resolve()), 'Root escapes disposable workspace')
    result = []
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink() and not path.is_junction(), f'Reparse point: {path}')
        if path.is_file():
            result.append(dict(path=path.relative_to(root).as_posix(), bytes=path.stat().st_size, sha256=sha(path)))
    return result


def prepare():
    require(not WORK.exists(), 'Use the retained prepared roots; do not overwrite them.')
    subject = json.loads((SUBJECT / 'subject.json').read_text())
    for item in subject['files']:
        require(sha(SUBJECT / item['path']) == item['sha256'], 'Frozen subject hash mismatch')
    # Verify the original disposable evidence before copying; never mutate it.
    for label in ('a', 'b'):
        source = BENCH / 'manual' / f'root-{label}'
        before = json.loads((BENCH / 'manual' / f'root-{label}-before.json').read_text())
        require({p.relative_to(source).as_posix() for p in source.rglob('*') if p.is_file()} == {r['path'] for r in before}, 'Original manual file set changed')
        for item in before:
            path = source / item['path']
            require(not path.is_symlink() and not path.is_junction(), 'Original manual reparse point')
            require(sha(path) == item['sha256'], 'Original manual hash changed')
    WORK.mkdir()
    for label in ('a', 'b'):
        shutil.copytree(BENCH / 'manual' / f'root-{label}', WORK / f'root-{label}')
        save(WORK / f'root-{label}-before.json', inventory(WORK / f'root-{label}'))
    (WORK / 'webview-profile').mkdir()
    config = ('[recording]\nprofile = "auto"\ncodec = "auto"\n\n[storage]\noutput_path = '
              + json.dumps(str(WORK / 'root-a'))
              + '\nauto_delete_days = 0\n\n[app]\nautostart = false\nhevc_playback_supported = false\n')
    (WORK / 'config.toml').write_text(config, encoding='utf-8')
    save(WORK / 'preparation.json', dict(utc=now(), binary_sha256=sha(APP), source_evidence_unchanged=True,
        roots={'a': {'games': 3, 'clips': 10}, 'b': {'games': 1, 'clips': 1}}))
    print(f'Prepared disposable copies only: {WORK}')


def windows():
    require(os.name == 'nt', 'Windows required')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]; kernel.OpenProcess.restype = w.HANDLE
    kernel.CloseHandle.argtypes = [w.HANDLE]
    kernel.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)]
    kernel.GetProcessTimes.argtypes = [w.HANDLE] + [ctypes.POINTER(w.FILETIME)] * 4
    kernel.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]; kernel.WaitForSingleObject.restype = w.DWORD
    return kernel


def identity(kernel, handle):
    buffer = ctypes.create_unicode_buffer(32768); length = w.DWORD(len(buffer))
    if not kernel.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(length)):
        raise ctypes.WinError(ctypes.get_last_error())
    times = [w.FILETIME() for _ in range(4)]
    if not kernel.GetProcessTimes(handle, *(ctypes.byref(t) for t in times)):
        raise ctypes.WinError(ctypes.get_last_error())
    return Path(buffer.value).resolve(), (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime


def launch():
    require((WORK / 'preparation.json').exists(), 'Run prepare first.')
    require(not (WORK / 'launch.json').exists(), 'A launch is already retained; do not overwrite or launch twice.')
    require(sha(APP) == json.loads((WORK / 'preparation.json').read_text())['binary_sha256'], 'Normal binary changed')
    env = os.environ.copy()
    for key in list(env):
        if key.startswith('LEAGUE_REPLAY_BENCHMARK') or key in ('LEAGUE_REPLAY_OUTPUT_PATH', 'QUEUEBACK_MEDIA_RUNTIME_DIR'):
            env.pop(key)
    env['LEAGUE_REPLAY_CONFIG'] = str(WORK / 'config.toml')
    env['WEBVIEW2_USER_DATA_FOLDER'] = str(WORK / 'webview-profile')
    with (WORK/'app.stdout.log').open('xb') as stdout, (WORK/'app.stderr.log').open('xb') as stderr:
        app = subprocess.Popen([str(APP)], cwd=REPO, env=env, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr)
    kernel = windows(); handle = kernel.OpenProcess(0x1000, False, app.pid)
    require(handle, 'Could not verify launched app identity')
    try:
        path, created = identity(kernel, handle)
        require(path == APP.resolve(), 'Launched app path mismatch')
        save(WORK/'launch.json', dict(utc=now(), pid=app.pid, creation_filetime=created,
            binary_sha256=sha(APP), normal_build=True, dedicated_config=True))
    finally:
        kernel.CloseHandle(handle)
    print(f'Normal disposable app PID {app.pid}. A={WORK / "root-a"}; B={WORK / "root-b"}')


def children(kernel, parent):
    class Entry(ctypes.Structure):
        _fields_ = [('dwSize',w.DWORD),('cntUsage',w.DWORD),('pid',w.DWORD),('heap',ctypes.c_size_t),
            ('module',w.DWORD),('threads',w.DWORD),('parent',w.DWORD),('priority',w.LONG),('flags',w.DWORD),('exe',w.WCHAR*260)]
    kernel.CreateToolhelp32Snapshot.argtypes=[w.DWORD,w.DWORD]; kernel.CreateToolhelp32Snapshot.restype=w.HANDLE
    kernel.Process32FirstW.argtypes=[w.HANDLE,ctypes.POINTER(Entry)]
    kernel.Process32NextW.argtypes=[w.HANDLE,ctypes.POINTER(Entry)]
    snap=kernel.CreateToolhelp32Snapshot(2,0)
    if snap == ctypes.c_void_p(-1).value: raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry=Entry(); entry.dwSize=ctypes.sizeof(entry)
        present=kernel.Process32FirstW(snap,ctypes.byref(entry))
        while present:
            if entry.parent == parent and entry.exe.lower() == 'ffprobe.exe': yield entry.pid
            present=kernel.Process32NextW(snap,ctypes.byref(entry))
    finally:
        kernel.CloseHandle(snap)


MAX_MANUAL_HOLD_SECONDS = 30


def hold(label, seconds):
    """Hold one verified direct optional child; app remains responsive; no input injection."""
    require(1 <= seconds <= MAX_MANUAL_HOLD_SECONDS,
            f'Hold duration must be between one and {MAX_MANUAL_HOLD_SECONDS} seconds')
    expected_probe = PROBE.resolve()
    observations = dict(polls=0, candidates=0, open_errors=0, identity_errors=0,
        path_mismatches=0, exited_before_suspend=0, suspend_errors=0)
    failures = []

    def failure(stage, pid, code):
        if len(failures) < 8:
            failures.append(dict(stage=stage, pid=pid, code=code))

    receipt=json.loads((WORK/'launch.json').read_text())
    kernel=windows(); parent=kernel.OpenProcess(0x1000 | 0x100000,False,receipt['pid'])
    require(parent, 'Could not open dedicated app identity')
    try:
        require(identity(kernel,parent) == (APP.resolve(),receipt['creation_filetime']), 'Dedicated parent identity changed')
        require(kernel.WaitForSingleObject(parent,0) == 258, 'Dedicated app has exited')
        nt=ctypes.WinDLL('ntdll')
        nt.NtSuspendProcess.argtypes=[w.HANDLE]; nt.NtSuspendProcess.restype=w.LONG
        nt.NtResumeProcess.argtypes=[w.HANDLE]; nt.NtResumeProcess.restype=w.LONG
        log=WORK/f'hold-{label}.json'
        require(not log.exists() and not (WORK/f'hold-{label}.started.json').exists(), 'Use a new label for each attempt.')
        print('ARMED for 60 seconds. Click Retry duration in the app now. '
              'Wait for HOLDING before the folder switch or other test action.',flush=True)
        deadline=time.monotonic()+60
        while time.monotonic() < deadline:
            require(kernel.WaitForSingleObject(parent,0) == 258, 'Dedicated app has exited')
            observations['polls'] += 1
            for pid in children(kernel,receipt['pid']):
                observations['candidates'] += 1
                handle=kernel.OpenProcess(0x0800 | 0x1000 | 0x100000,False,pid)
                if not handle:
                    observations['open_errors'] += 1
                    failure('OpenProcess', pid, ctypes.get_last_error())
                    continue
                suspended=False
                try:
                    try:
                        path,created=identity(kernel,handle)
                    except OSError as error:
                        observations['identity_errors'] += 1
                        failure('process_identity', pid, getattr(error, 'winerror', None) or error.errno)
                        continue # The short-lived process may have exited after enumeration.
                    if path != expected_probe:
                        observations['path_mismatches'] += 1
                        continue
                    if kernel.WaitForSingleObject(handle,0) != 258:
                        observations['exited_before_suspend'] += 1
                        continue
                    suspend_status = int(nt.NtSuspendProcess(handle))
                    if suspend_status != 0:
                        observations['suspend_errors'] += 1
                        failure('NtSuspendProcess', pid, suspend_status)
                        continue
                    suspended=True; start=time.monotonic()
                    event=dict(label=label,app_pid=receipt['pid'],child_pid=pid,creation_filetime=created,
                        child_path_verified=True,parent_verified=True,started_utc=now(),hold_limit_seconds=seconds,
                        observations=observations,failures=failures)
                    save(WORK/f'hold-{label}.started.json',event)
                    print(f'HOLDING optional ffprobe for at most {seconds}s: perform the rehearsed UI action NOW.',flush=True)
                    while time.monotonic()-start < seconds and kernel.WaitForSingleObject(handle,0) == 258:
                        time.sleep(0.02)
                    exited=kernel.WaitForSingleObject(handle,0)==0
                    resume=None if exited else int(nt.NtResumeProcess(handle))
                    suspended=not exited and resume!=0
                    event.update(completed_utc=now(),actual_hold_seconds=time.monotonic()-start,
                        resume_status=resume,child_exited_during_hold=kernel.WaitForSingleObject(handle,0)==0,
                        ui_action_observed=False)
                    save(log,event)
                    require(not suspended, f'Child resume failed with NTSTATUS {resume}; recovery resume will run in finally')
                    print('Hold ended; record what the UI actually did. This receipt alone is not a manual pass.'); return True
                finally:
                    if suspended: nt.NtResumeProcess(handle)
                    kernel.CloseHandle(handle)
            time.sleep(0.005)
        save(log,dict(label=label,utc=now(),status='not-observed',ui_action_observed=False,
            observations=observations,failures=failures))
        print('NO HOLD: the watcher did not capture a verified live probe. '
              'Leave the app open and report this summary; no overlap pass is recorded.',flush=True)
        print(json.dumps(dict(observations=observations,failures=failures)),flush=True)
        return False
    finally:
        kernel.CloseHandle(parent)


def audit(label, note):
    result=dict(utc=now(),label=label,user_observation=note,roots={})
    for root_label in ('a','b'):
        root=WORK/f'root-{root_label}'
        before={x['path']:x for x in json.loads((WORK/f'root-{root_label}-before.json').read_text())}
        after={x['path']:x for x in inventory(root)}
        result['roots'][root_label]=dict(files=list(after.values()),
            deleted=sorted(before.keys()-after.keys()),added=sorted(after.keys()-before.keys()),
            changed=sorted(k for k in before.keys() & after.keys() if before[k]['sha256'] != after[k]['sha256']),
            game_directories=sorted(p.name for p in (root/'games').iterdir() if p.is_dir()),
            clip_files=sorted(p.name for p in (root/'clips').glob('*.mp4')))
        result['roots'][root_label]['game_metadata'] = {
            p.parent.name: {key: json.loads(p.read_text())[key]
                for key in ('saved','local_player_champion','recorded_at')}
            for p in (root/'games').glob('*/metadata.json')
        }
        for relative in after.keys()-before.keys():
            if relative.startswith('clips/') and relative.endswith('.mp4'):
                path=root/relative
                probe=subprocess.run([str(PROBE),'-v','error','-show_entries','format=duration:stream=codec_type,codec_name','-of','json',str(path)],capture_output=True,text=True,timeout=30)
                decode=subprocess.run([str(FFMPEG),'-v','error','-threads','1','-i',str(path),'-f','null','NUL'],capture_output=True,text=True,timeout=120)
                result['roots'][root_label].setdefault('export_validation',[]).append(dict(file=relative,
                    probe_exit=probe.returncode,probe_stdout=probe.stdout,probe_stderr=probe.stderr,
                    decode_exit=decode.returncode,decode_stderr=decode.stderr))
    save(WORK/f'audit-{label}.json',result)
    print(json.dumps({k:{n:v for n,v in row.items() if n!='files'} for k,row in result['roots'].items()},indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['prepare','launch','hold','audit'])
    parser.add_argument('--label',default='final',help='Unique receipt label; never overwritten')
    parser.add_argument('--seconds',type=int,default=8)
    parser.add_argument('--note',default='',help='Exact user-observed outcome, not inferred by this helper')
    args=parser.parse_args()
    require(args.label and all(c.isalnum() or c in '-_' for c in args.label), 'Safe receipt label required')
    require(WORK.resolve().is_relative_to(REPO.resolve()), 'Disposable workspace escapes repository')
    for path in (WORK, *WORK.parents):
        require(not path.is_symlink() and not path.is_junction(), f'Reparse ancestor: {path}')
    if args.command=='prepare': prepare()
    elif args.command=='launch': launch()
    elif args.command=='hold':
        if not hold(args.label,args.seconds): raise SystemExit(1)
    else: audit(args.label,args.note)


if __name__=='__main__':
    main()
