"""Bounded Windows production-mux resource measurement with prerecorded H.264."""

import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
from pathlib import Path
import queue
import subprocess
import threading
import time


ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / "build/perf/qb-perf-006"
KERNEL = c.WinDLL("kernel32", use_last_error=True)
PSAPI = c.WinDLL("psapi", use_last_error=True)


class IoCounters(c.Structure):
    _fields_ = [(name, c.c_ulonglong) for name in (
        "read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes"
    )]


class Memory(c.Structure):
    _fields_ = [("cb", w.DWORD), ("faults", w.DWORD)] + [
        (name, c.c_size_t) for name in (
            "peak_working", "working", "peak_paged", "paged", "peak_nonpaged",
            "nonpaged", "pagefile", "peak_pagefile", "private"
        )
    ]


class ProcessEntry(c.Structure):
    _fields_ = [
        ("size", w.DWORD), ("usage", w.DWORD), ("pid", w.DWORD),
        ("heap", c.c_size_t), ("module", w.DWORD), ("threads", w.DWORD),
        ("parent", w.DWORD), ("priority", w.LONG), ("flags", w.DWORD),
        ("name", w.WCHAR * 260),
    ]


KERNEL.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
KERNEL.OpenProcess.restype = w.HANDLE
KERNEL.CloseHandle.argtypes = [w.HANDLE]
KERNEL.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
KERNEL.GetProcessIoCounters.argtypes = [w.HANDLE, c.POINTER(IoCounters)]
KERNEL.GetProcessHandleCount.argtypes = [w.HANDLE, c.POINTER(w.DWORD)]
KERNEL.QueryProcessCycleTime.argtypes = [w.HANDLE, c.POINTER(c.c_ulonglong)]
KERNEL.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
KERNEL.CreateToolhelp32Snapshot.restype = w.HANDLE
KERNEL.Process32FirstW.argtypes = [w.HANDLE, c.POINTER(ProcessEntry)]
KERNEL.Process32NextW.argtypes = [w.HANDLE, c.POINTER(ProcessEntry)]
PSAPI.GetProcessMemoryInfo.argtypes = [w.HANDLE, c.POINTER(Memory), w.DWORD]


def checked(success):
    if not success:
        raise c.WinError(c.get_last_error())


def child_pid(parent):
    snapshot = KERNEL.CreateToolhelp32Snapshot(2, 0)
    if snapshot == c.c_void_p(-1).value:
        raise c.WinError(c.get_last_error())
    try:
        entry = ProcessEntry()
        entry.size = c.sizeof(entry)
        more = KERNEL.Process32FirstW(snapshot, c.byref(entry))
        while more:
            if entry.parent == parent and entry.name.lower() == "ffmpeg.exe":
                return entry.pid
            more = KERNEL.Process32NextW(snapshot, c.byref(entry))
    finally:
        KERNEL.CloseHandle(snapshot)
    raise RuntimeError("probe has no FFmpeg child")


def sample(handle):
    times = [w.FILETIME() for _ in range(4)]
    checked(KERNEL.GetProcessTimes(handle, *(c.byref(value) for value in times)))
    ticks = lambda value: value.dwLowDateTime + (value.dwHighDateTime << 32)
    io = IoCounters()
    checked(KERNEL.GetProcessIoCounters(handle, c.byref(io)))
    memory = Memory()
    memory.cb = c.sizeof(memory)
    checked(PSAPI.GetProcessMemoryInfo(handle, c.byref(memory), memory.cb))
    handles = w.DWORD()
    checked(KERNEL.GetProcessHandleCount(handle, c.byref(handles)))
    cycles = c.c_ulonglong()
    checked(KERNEL.QueryProcessCycleTime(handle, c.byref(cycles)))
    return {
        "cpu_100ns": ticks(times[2]) + ticks(times[3]), "private": memory.private,
        "cpu_cycles": cycles.value,
        "working": memory.working, "handles": handles.value,
        **{name: getattr(io, name) for name, _ in io._fields_},
    }


def run(command, log):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            creationflags=subprocess.CREATE_NO_WINDOW, timeout=600)
    log.write_bytes(result.stdout)
    if result.returncode:
        raise RuntimeError(f"command failed, see {log}")
    return result.stdout


def sha256(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def generate(directory, ffmpeg, ffprobe, bitrate):
    directory.mkdir()
    source = directory / "source.h264"
    run([str(ffmpeg), "-v", "error", "-f", "lavfi", "-i",
         "testsrc2=size=1920x1080:rate=60:duration=4", "-c:v", "libx264",
         "-preset", "fast", "-threads", "2", "-profile:v", "high", "-bf", "0",
         "-g", "120", "-b:v", f"{bitrate}M", "-maxrate", f"{bitrate}M",
         "-bufsize", f"{bitrate * 2}M", "-x264-params",
         "repeat-headers=1:aud=1:scenecut=0:nal-hrd=cbr:force-cfr=1",
         "-f", "h264", str(source)], directory / "generate.log")
    probe = json.loads(run([str(ffprobe), "-v", "error", "-show_packets",
                           "-show_entries", "packet=pos,size", "-of", "json", str(source)],
                          directory / "packets.json"))
    packets = probe["packets"]
    if len(packets) != 240:
        raise RuntimeError("source does not have 240 access units")
    offset = 0
    sizes = []
    for packet in packets:
        if int(packet["pos"]) != offset:
            raise RuntimeError("source packets are not contiguous")
        sizes.append(int(packet["size"]))
        offset += sizes[-1]
    if offset != source.stat().st_size:
        raise RuntimeError("source packet bytes do not reconcile")
    (directory / "sizes.json").write_text(json.dumps(sizes))


def measure(directory, source, runtime, seconds, mode):
    directory.mkdir()
    source_hash = sha256(source / "source.h264")
    executable = ROOT / "recorder/target/release/examples/mux_resource_probe.exe"
    command = [str(executable), str(runtime), str(source / "source.h264"),
               str(source / "sizes.json"), str(directory), str(seconds), mode]
    events = queue.Queue()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                               text=True, creationflags=subprocess.CREATE_NO_WINDOW)
    def read_lines():
        for line in process.stdout:
            events.put((time.perf_counter(), line.rstrip()))
    reader = threading.Thread(target=read_lines)
    reader.start()
    handles = {}
    samples, messages = [], []
    start = time.perf_counter()
    active = None
    draining = False
    try:
        while process.poll() is None:
            while not events.empty():
                stamp, line = events.get_nowait()
                messages.append({"elapsed": stamp - start, "line": line})
                if line.startswith("READY "):
                    mux_pid = child_pid(process.pid)
                    for label, pid in (("recorder", process.pid), ("ffmpeg", mux_pid)):
                        handle = KERNEL.OpenProcess(0x410, False, pid)
                        checked(handle)
                        handles[label] = handle
                if line == "ACTIVE":
                    active = stamp
                if line == "DRAIN":
                    draining = True
            if handles and not draining:
                before = time.perf_counter()
                row = {label: sample(handle) for label, handle in handles.items()}
                row["elapsed"] = (before + time.perf_counter()) / 2 - start
                row["active_elapsed"] = None if active is None else before - active
                output = directory / "video.partial.mp4"
                row["output_file_length"] = output.stat().st_size if output.exists() else 0
                samples.append(row)
            if time.perf_counter() - start > seconds + 60:
                raise TimeoutError("mux probe exceeded bounded workload deadline")
            time.sleep(0.5)
        reader.join(timeout=5)
        while not events.empty():
            stamp, line = events.get_nowait()
            messages.append({"elapsed": stamp - start, "line": line})
        (directory / "resources.json").write_text(json.dumps(samples))
        (directory / "process.json").write_text(json.dumps({
            "command": command, "messages": messages, "returncode": process.returncode,
            "source_sha256": source_hash,
            "source_sha256_after": sha256(source / "source.h264"),
            "executable_sha256": sha256(executable),
            "ffmpeg_sha256": sha256(runtime / "bin/ffmpeg.exe"),
            "ffprobe_sha256": sha256(runtime / "bin/ffprobe.exe"),
        }, indent=2))
        if process.returncode:
            raise RuntimeError(f"mux probe failed, see {directory / 'process.json'}")
        if sha256(source / "source.h264") != source_hash:
            raise RuntimeError("source changed during measurement")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        reader.join(timeout=5)
        for handle in handles.values():
            KERNEL.CloseHandle(handle)
    ffmpeg, ffprobe = runtime / "bin/ffmpeg.exe", runtime / "bin/ffprobe.exe"
    run([str(ffprobe), "-v", "error", "-count_frames", "-show_entries",
         "stream=codec_type,codec_name,profile,width,height,nb_read_frames,avg_frame_rate,start_time,duration:format=duration,size",
         "-of", "json", str(directory / "video.partial.mp4")], directory / "media.json")
    run([str(ffmpeg), "-v", "error", "-xerror", "-threads", "1", "-i",
         str(directory / "video.partial.mp4"), "-f", "null", "NUL"], directory / "decode.log")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, default=ROOT / "build/media-runtime/windows-x86_64")
    parser.add_argument("--name", required=True)
    parser.add_argument("--seconds", type=int, default=120)
    parser.add_argument("--mode", choices=("video", "silence", "pcm"), default="pcm")
    parser.add_argument("--bitrate", type=int, choices=(12, 18), default=12)
    args = parser.parse_args()
    if not 10 <= args.seconds <= 600 or not args.name.replace("-", "").isalnum():
        parser.error("use a simple unique run name and 10-600 seconds")
    BASE.mkdir(parents=True, exist_ok=True)
    source = BASE / f"source-{args.bitrate}"
    runtime = args.runtime.resolve()
    if not source.exists():
        generate(source, runtime / "bin/ffmpeg.exe", runtime / "bin/ffprobe.exe", args.bitrate)
    directory = BASE / args.name
    measure(directory, source, runtime, args.seconds, args.mode)
    from analyze_mux import analyze
    analyze(directory)
    print(f"Completed {BASE / args.name}", flush=True)


if __name__ == "__main__":
    main()
