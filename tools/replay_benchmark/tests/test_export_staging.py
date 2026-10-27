from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from test_powershell_contract import RUN, run_script


# Load the real runner functions without its launch/finalization body. Only media
# subprocess results are controlled; paths, enumeration and hashes remain real.
HARNESS = r'''
param([string]$Runner, [string]$Root, [string]$Case)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$tokens = $null; $parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($Runner, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw "Runner parse failed: $parseErrors" }
$functions = $ast.EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.FunctionDefinitionAst] }
Invoke-Expression (($functions | ForEach-Object { $_.Extent.Text }) -join "`n")
$sentinel = Join-Path $Root '.chronobreak-replay-benchmark'
$library = Join-Path $sentinel 'library'
$clips = Join-Path $library 'clips'
[void](New-Item -ItemType Directory -Path $clips -Force)
$staging = Join-Path $clips '.chronobreak-export'
$lease = Join-Path $staging '.lease'
$held = $null
function Add-IdleStaging {
    [void](New-Item -ItemType Directory -Path $staging)
    [System.IO.File]::WriteAllBytes($lease, [byte[]]@())
}
function Get-StagingBytes {
    $result = @{}
    if (Test-Path -LiteralPath $staging) {
        foreach ($entry in Get-ChildItem -LiteralPath $staging -Recurse -Force) {
            $result[$entry.FullName] = $(if ($entry.PSIsContainer) { 'directory' } else { Get-Sha256 $entry.FullName })
        }
    }
    return ($result | ConvertTo-Json -Compress)
}
if ($Case -in @('subsequent', 'preexisting-foreign', 'reused-output')) {
    Add-IdleStaging
    if ($Case -ne 'reused-output') {
        $batch = Join-Path $staging '0123456789abcdef0123456789abcdef'
        [void](New-Item -ItemType Directory -Path $batch)
        $owner = if ($Case -eq 'subsequent') { '{"version":1,"batch_id":"0123456789abcdef0123456789abcdef","destinations":["1700000000_1"]}' } else { 'foreign-malformed' }
        [System.IO.File]::WriteAllText((Join-Path $batch 'owner.json'), $owner)
        [System.IO.File]::WriteAllText((Join-Path $batch '0.mp4'), 'retained-stage-bytes')
    }
}
$video = Join-Path $clips '1700000000_2.mp4'
$thumbnail = Join-Path $clips '1700000000_2.jpg'
if ($Case -eq 'reused-output') { [System.IO.File]::WriteAllText($video, 'old') }
$before = Get-ClipDirectorySnapshot -SentinelRoot $sentinel -LibraryRoot $library
$preservedBefore = Get-StagingBytes
if ($Case -notin @('subsequent', 'preexisting-foreign', 'reused-output', 'no-staging', 'foreign-dir', 'wrong-case-parent')) { Add-IdleStaging }
switch ($Case) {
    'foreign-dir' { [void](New-Item -ItemType Directory -Path (Join-Path $clips 'foreign')) }
    'wrong-case-parent' {
        [void](New-Item -ItemType Directory -Path (Join-Path $clips '.Chronobreak-export'))
        [System.IO.File]::WriteAllText((Join-Path $clips '.Chronobreak-export/.lease'), '')
    }
    'missing-lease' { Remove-Item -LiteralPath $lease }
    'nonempty-lease' { [System.IO.File]::WriteAllText($lease, 'foreign') }
    'directory-lease' { Remove-Item -LiteralPath $lease; [void](New-Item -ItemType Directory -Path $lease) }
    'wrong-case-lease' { Rename-Item -LiteralPath $lease -NewName '.LEASE' }
    'extra-file' { [System.IO.File]::WriteAllText((Join-Path $staging 'foreign'), 'foreign') }
    'hidden-extra' {
        $extra = Join-Path $staging 'hidden'
        [System.IO.File]::WriteAllText($extra, 'foreign')
        [System.IO.File]::SetAttributes($extra, [System.IO.FileAttributes]::Hidden)
    }
    'batch' { [void](New-Item -ItemType Directory -Path (Join-Path $staging '0123456789abcdef0123456789abcdef')) }
    'owner' { [System.IO.File]::WriteAllText((Join-Path $staging 'owner.json'), '{}') }
    'held-lease' { $held = [System.IO.File]::Open($lease, 'Open', 'ReadWrite', 'ReadWrite') }
    'staging-junction' {
        Remove-Item -LiteralPath $lease
        Remove-Item -LiteralPath $staging
        $target = Join-Path $Root 'foreign-target'
        [void](New-Item -ItemType Directory -Path $target)
        [System.IO.File]::WriteAllText((Join-Path $target '.lease'), '')
        [void](New-Item -ItemType Junction -Path $staging -Target $target)
    }
    'lease-junction' {
        Remove-Item -LiteralPath $lease
        $target = Join-Path $Root 'foreign-target'
        [void](New-Item -ItemType Directory -Path $target)
        [void](New-Item -ItemType Junction -Path $lease -Target $target)
    }
    'enumeration-error' {
        function Get-ChildItem {
            param([string]$LiteralPath, [switch]$Force, [object]$ErrorAction)
            if ($LiteralPath -eq $staging) { throw 'controlled enumeration refusal' }
            Microsoft.PowerShell.Management\Get-ChildItem -LiteralPath $LiteralPath -Force:$Force -ErrorAction Stop
        }
    }
    'foreign-output' { [System.IO.File]::WriteAllText((Join-Path $clips 'foreign.txt'), 'foreign') }
    'root-mismatch' { $before.clips_root = Join-Path $library 'other' }
}
[System.IO.File]::WriteAllText($video, 'controlled-video')
[System.IO.File]::WriteAllText($thumbnail, 'controlled-thumbnail')
function Invoke-NativeTool {
    param([string]$FilePath, [string[]]$Arguments, [int]$FiniteTimeoutSeconds, [string]$Label)
    if ($FilePath -eq 'probe') {
        $codec = if ($Case -eq 'bad-codec') { 'hevc' } else { 'h264' }
        $probe = @{streams=@(@{index=0;codec_type='video';codec_name=$codec},@{index=1;codec_type='audio';codec_name='aac'});format=@{duration='5.000';size=(Get-Item -LiteralPath $video).Length.ToString()}}
        return @{exit_code=0;stdout=($probe | ConvertTo-Json -Depth 8);stderr=''}
    }
    return @{exit_code=$(if ($Case -eq 'bad-decode') {1} else {0});stdout='';stderr=''}
}
$events = Join-Path $Root 'events.jsonl'
$outputs = @(@{filename='1700000000_2';preset='horizontal';file_size_bytes=(Get-Item -LiteralPath $video).Length})
if ($Case -eq 'missing-output') { Remove-Item -LiteralPath $thumbnail }
$event = @{schema_version=1;run_id='staging-test';scenario_id='export';trial_id='one';kind='export_completed';payload=@{outputs=$outputs}}
[System.IO.File]::WriteAllText($events, ($event | ConvertTo-Json -Depth 10 -Compress))
$manifest = [pscustomobject]@{run_id='staging-test';fixtures=@([pscustomobject]@{id='fixture';game_timestamp='1700000000'});media_tools=[pscustomobject]@{ffprobe=@{path='probe'};ffmpeg=@{path='decode'}}}
$scenario = [pscustomobject]@{id='export';trial_id='one';fixture_ids=@('fixture');export_presets=@('horizontal');expected_duration_ms=5000;expected_video_codec='h264';expected_audio_codec='aac'}
try {
    $report = Invoke-ExportValidation -ManifestValue $manifest -Scenario $scenario -BeforeSnapshot $before -EventsPath $events -SentinelRoot $sentinel -LibraryRoot $library -FiniteTimeoutSeconds 5
    $preserved = if ($Case -in @('subsequent','preexisting-foreign')) { $preservedBefore -eq (Get-StagingBytes) } else { $true }
    @{report=$report;preexisting_preserved=$preserved} | ConvertTo-Json -Depth 30 -Compress
}
finally { if ($null -ne $held) { $held.Dispose() } }
'''


class ExportStagingValidationTests(unittest.TestCase):
    def validate(self, case: str) -> dict:
        with tempfile.TemporaryDirectory(prefix="qb-export-validator-") as directory:
            root = Path(directory)
            script = root / "check.ps1"
            script.write_text(HARNESS, encoding="utf-8")
            result = run_script(script, "-Runner", str(RUN), "-Root", str(root), "-Case", case)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return json.loads(result.stdout)

    def test_first_use_accepts_only_idle_residue_and_reports_exact_media_entries(self):
        result = self.validate("first-use")
        report = result["report"]
        self.assertTrue(report["all_valid"], report["errors"])
        self.assertEqual(len(report["newly_created_entries"]), 2)
        self.assertTrue(all(e["kind"] == "file" for e in report["newly_created_entries"]))
        self.assertEqual(report["staging_residue"]["relative_path"], r"clips\.chronobreak-export")
        self.assertEqual(report["staging_residue"]["lease_size_bytes"], 0)
        self.assertTrue(report["staging_residue"]["exclusive_access"])

    def test_no_residue_and_preexisting_staging_keep_existing_semantics(self):
        for case in ("no-staging", "subsequent", "preexisting-foreign"):
            with self.subTest(case=case):
                result = self.validate(case)
                self.assertTrue(result["report"]["all_valid"], result["report"]["errors"])
                self.assertTrue(result["preexisting_preserved"])

    def test_unsafe_or_foreign_new_entries_fail_closed(self):
        for case in ("foreign-dir", "wrong-case-parent", "missing-lease", "nonempty-lease",
                     "directory-lease", "wrong-case-lease", "extra-file", "hidden-extra",
                     "batch", "owner", "held-lease", "staging-junction", "lease-junction",
                     "enumeration-error", "foreign-output", "root-mismatch"):
            with self.subTest(case=case):
                report = self.validate(case)["report"]
                self.assertFalse(report["all_valid"], report)
                self.assertTrue(report["errors"])

    def test_idle_residue_does_not_mask_output_validation_failures(self):
        for case in ("bad-codec", "bad-decode", "missing-output", "reused-output"):
            with self.subTest(case=case):
                report = self.validate(case)["report"]
                self.assertFalse(report["all_valid"], report)
                self.assertTrue(report["errors"])


if __name__ == "__main__":
    unittest.main()
