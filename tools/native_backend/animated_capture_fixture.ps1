param(
    [int]$DurationSeconds = 300,
    [string]$ReadyFile = '',
    [ValidateRange(320, 3840)]
    [int]$Width = 1280,
    [ValidateRange(240, 2160)]
    [int]$Height = 720,
    [switch]$Borderless
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
[System.Windows.Forms.Application]::SetCompatibleTextRenderingDefault($false)

$form = [System.Windows.Forms.Form]::new()
$form.Text = 'Chronobreak animated non-League capture fixture'
$form.ClientSize = [System.Drawing.Size]::new($Width, $Height)
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$form.Location = if ($Borderless) {
    [System.Drawing.Point]::new(0, 0)
} else {
    [System.Drawing.Point]::new(100, 100)
}
if ($Borderless) {
    $form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
}
$form.BackColor = [System.Drawing.Color]::Black
$doubleBuffered = $form.GetType().GetProperty(
    'DoubleBuffered',
    [System.Reflection.BindingFlags]'Instance,NonPublic'
)
$doubleBuffered.SetValue($form, $true)

$script:fixtureFrame = 0L
$clock = [System.Diagnostics.Stopwatch]::StartNew()
$timer = [System.Windows.Forms.Timer]::new()
$timer.Interval = 16

$form.Add_Paint({
    param($sender, $eventArgs)

    $graphics = $eventArgs.Graphics
    $width = $sender.ClientSize.Width
    $height = $sender.ClientSize.Height
    $x = [int](($script:fixtureFrame * 13) % [Math]::Max(1, $width - 220))
    $y = [int](($script:fixtureFrame * 7) % [Math]::Max(1, $height - 160))
    $phase = [int]($script:fixtureFrame % 255)
    $graphics.Clear([System.Drawing.Color]::FromArgb(8, 10, 18))

    $gridPen = [System.Drawing.Pen]::new(
        [System.Drawing.Color]::FromArgb(45, 60, 90),
        1
    )
    try {
        for ($gridX = 0; $gridX -lt $width; $gridX += 32) {
            $graphics.DrawLine($gridPen, $gridX, 0, $gridX, $height)
        }
        for ($gridY = 0; $gridY -lt $height; $gridY += 32) {
            $graphics.DrawLine($gridPen, 0, $gridY, $width, $gridY)
        }
    } finally {
        $gridPen.Dispose()
    }

    $blockBrush = [System.Drawing.SolidBrush]::new(
        [System.Drawing.Color]::FromArgb(255, $phase, 255 - $phase, 180)
    )
    try {
        $graphics.FillRectangle($blockBrush, $x, $y, 220, 160)
    } finally {
        $blockBrush.Dispose()
    }

    $font = [System.Drawing.Font]::new(
        'Consolas',
        24,
        [System.Drawing.FontStyle]::Bold,
        [System.Drawing.GraphicsUnit]::Pixel
    )
    $textBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::White)
    try {
        $graphics.DrawString(
            "frame $script:fixtureFrame",
            $font,
            $textBrush,
            24,
            24
        )
    } finally {
        $textBrush.Dispose()
        $font.Dispose()
    }
})

$timer.Add_Tick({
    $script:fixtureFrame++
    if ($clock.Elapsed.TotalSeconds -ge $DurationSeconds) {
        $form.Close()
    } else {
        $form.Invalidate()
    }
})
$form.Add_Shown({
    if ($ReadyFile) {
        [System.IO.File]::WriteAllText(
            $ReadyFile,
            "$PID|$($form.Handle.ToInt64())"
        )
    }
    $timer.Start()
})
$form.Add_FormClosed({ $timer.Stop() })

try {
    [System.Windows.Forms.Application]::Run($form)
} finally {
    $timer.Dispose()
    $form.Dispose()
}
