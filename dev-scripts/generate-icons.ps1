param(
    [string]$RepositoryRoot = (Split-Path $PSScriptRoot -Parent)
)

Add-Type -AssemblyName System.Drawing

$size = 512
$bitmap = [System.Drawing.Bitmap]::new($size, $size)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.Clear([System.Drawing.Color]::FromArgb(17, 22, 28))

$white = [System.Drawing.Color]::FromArgb(244, 247, 248)
$teal = [System.Drawing.Color]::FromArgb(45, 212, 191)
$coral = [System.Drawing.Color]::FromArgb(255, 107, 95)
$clockPen = [System.Drawing.Pen]::new($white, 12)
$clockPen.StartCap = $clockPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$graphics.DrawEllipse($clockPen, 70, 70, 372, 372)

$tickPen = [System.Drawing.Pen]::new($white, 11)
$tickPen.StartCap = $tickPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
foreach ($angle in 0..7) {
    $radians = ($angle * 45) * [Math]::PI / 180
    $x1 = 256 + [Math]::Sin($radians) * 174
    $y1 = 256 - [Math]::Cos($radians) * 174
    $x2 = 256 + [Math]::Sin($radians) * 203
    $y2 = 256 - [Math]::Cos($radians) * 203
    $graphics.DrawLine($tickPen, $x1, $y1, $x2, $y2)
}

$arcPen = [System.Drawing.Pen]::new($teal, 46)
$arcPen.StartCap = $arcPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$graphics.DrawArc($arcPen, 126, 126, 260, 260, 45, 270)

$handPen = [System.Drawing.Pen]::new($white, 17)
$handPen.StartCap = $handPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$graphics.DrawLine($handPen, 256, 256, 256, 175)
$graphics.DrawLine($handPen, 256, 256, 327, 297)

$coralBrush = [System.Drawing.SolidBrush]::new($coral)
$graphics.FillEllipse($coralBrush, 236, 236, 40, 40)
$graphics.FillEllipse($coralBrush, 342, 145, 21, 21)
$graphics.FillEllipse($coralBrush, 342, 346, 21, 21)

$output = Join-Path $RepositoryRoot "res\chronahle-icon.png"
$bitmap.Save($output, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
$clockPen.Dispose()
$tickPen.Dispose()
$arcPen.Dispose()
$handPen.Dispose()
$coralBrush.Dispose()

$targets = @(
    "res\icon.png",
    "res\icon_preview.png",
    "res\icon_unofficial.png",
    "android\app\src\main\res\drawable-nodpi\icon.png",
    "android\app\src\main\res\drawable-nodpi\icon_preview.png",
    "android\app\src\main\res\drawable-nodpi\icon_unofficial.png"
)
foreach ($target in $targets) {
    Copy-Item -LiteralPath $output -Destination (Join-Path $RepositoryRoot $target) -Force
}

Write-Output "Generated ChronaHLE icons from $output"
