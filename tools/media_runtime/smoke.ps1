[CmdletBinding()]
param(
    [string]$RuntimeRoot,
    [string]$ResultRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if ([string]::IsNullOrWhiteSpace($RuntimeRoot)) {
    $RuntimeRoot = Join-Path $repository "build\media-runtime\windows-x86_64"
}
$RuntimeRoot = [System.IO.Path]::GetFullPath($RuntimeRoot)
if ([string]::IsNullOrWhiteSpace($ResultRoot)) {
    $ResultRoot = Join-Path $repository ("build\media-runtime\smoke-" + (Get-Date -Format "yyyyMMdd-HHmmss") + "-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
}
$ResultRoot = [System.IO.Path]::GetFullPath($ResultRoot)

$ffmpeg = Join-Path $RuntimeRoot "bin\ffmpeg.exe"
$ffprobe = Join-Path $RuntimeRoot "bin\ffprobe.exe"
foreach ($tool in @($ffmpeg, $ffprobe)) {
    if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) { throw "Packaged tool is missing: $tool" }
}

New-Item -ItemType Directory -Force -Path $ResultRoot | Out-Null
Set-Content -LiteralPath (Join-Path $ResultRoot ".queueback-media-runtime-smoke") -Encoding ascii -Value "Dedicated QueueBack generated-media smoke fixture. Never user media."

$source = Join-Path $ResultRoot "source.mp4"
$clip = Join-Path $ResultRoot "clip.mp4"
$thumbnail = Join-Path $ResultRoot "clip.jpg"
$probeJson = Join-Path $ResultRoot "ffprobe.json"
$decodeLog = Join-Path $ResultRoot "decode.log"
$originalPath = $env:PATH
$sanitizedPath = "$env:SystemRoot\System32;$env:SystemRoot"

try {
    $env:PATH = $sanitizedPath
    if (Get-Command ffmpeg -ErrorAction SilentlyContinue) { throw "Sanitized PATH unexpectedly resolves ffmpeg." }
    if (Get-Command ffprobe -ErrorAction SilentlyContinue) { throw "Sanitized PATH unexpectedly resolves ffprobe." }

    & $ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=size=1280x720:rate=60:duration=4" -f lavfi -i "sine=frequency=880:sample_rate=48000:duration=4" -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -movflags +faststart $source
    if ($LASTEXITCODE -ne 0) { throw "Packaged fixture encode failed with exit code $LASTEXITCODE." }

    $probe = & $ffprobe -v error -show_entries "format=duration:stream=index,codec_type,codec_name,avg_frame_rate" -of json $source
    if ($LASTEXITCODE -ne 0) { throw "Packaged ffprobe failed with exit code $LASTEXITCODE." }
    [System.IO.File]::WriteAllText($probeJson, (($probe -join [Environment]::NewLine) + [Environment]::NewLine), [System.Text.UTF8Encoding]::new($false))
    $probeDocument = Get-Content -Raw -LiteralPath $probeJson | ConvertFrom-Json
    if (-not ($probeDocument.streams | Where-Object { $_.codec_type -eq "video" })) { throw "Fixture probe found no video stream." }
    if (-not ($probeDocument.streams | Where-Object { $_.codec_type -eq "audio" })) { throw "Fixture probe found no audio stream." }

    & $ffmpeg -hide_banner -loglevel error -y -ss 0.5 -t 2.5 -i $source -vf "scale=640:-2" -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac $clip
    if ($LASTEXITCODE -ne 0) { throw "Packaged clip export failed with exit code $LASTEXITCODE." }
    & $ffmpeg -hide_banner -loglevel error -y -ss 1 -i $clip -frames:v 1 -q:v 3 $thumbnail
    if ($LASTEXITCODE -ne 0) { throw "Packaged thumbnail generation failed with exit code $LASTEXITCODE." }

    $decodeOutput = & $ffmpeg -hide_banner -v error -i $clip -f null NUL 2>&1
    [System.IO.File]::WriteAllText($decodeLog, (($decodeOutput -join [Environment]::NewLine) + [Environment]::NewLine), [System.Text.UTF8Encoding]::new($false))
    if ($LASTEXITCODE -ne 0 -or -not [string]::IsNullOrWhiteSpace(($decodeOutput -join ""))) {
        throw "Packaged full decode reported an error."
    }

    $evidence = [ordered]@{
        schema_version = 1
        runtime_root = $RuntimeRoot
        ffmpeg = $ffmpeg
        ffprobe = $ffprobe
        path = $sanitizedPath
        system_ffmpeg_resolved = $false
        system_ffprobe_resolved = $false
        source_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash.ToLowerInvariant()
        clip_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $clip).Hash.ToLowerInvariant()
        thumbnail_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $thumbnail).Hash.ToLowerInvariant()
        decode_errors = 0
    }
    [System.IO.File]::WriteAllText((Join-Path $ResultRoot "smoke-result.json"), (($evidence | ConvertTo-Json -Depth 5) + [Environment]::NewLine), [System.Text.UTF8Encoding]::new($false))
}
finally {
    $env:PATH = $originalPath
}

Write-Output "Packaged media smoke passed at $ResultRoot"

