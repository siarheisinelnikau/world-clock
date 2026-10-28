Add-Type -AssemblyName System.Drawing
$outPath = Join-Path $PSScriptRoot 'app.ico'
$images = @()
foreach ($size in @(16, 24, 32, 48, 64, 128, 256)) {
    $bmp = [System.Drawing.Bitmap]::new($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    $bg = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(20, 29, 41))
    $mint = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(126, 232, 196), $size * 0.065)
    $mint.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $mint.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.FillEllipse($bg, $size * 0.02, $size * 0.02, $size * 0.96, $size * 0.96)
    $g.DrawEllipse($mint, $size * 0.19, $size * 0.19, $size * 0.62, $size * 0.62)
    $g.DrawLine($mint, $size * 0.5, $size * 0.34, $size * 0.5, $size * 0.51)
    $g.DrawLine($mint, $size * 0.5, $size * 0.51, $size * 0.64, $size * 0.59)
    $stream = [System.IO.MemoryStream]::new()
    $bmp.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $images += ,@($size, $stream.ToArray())
    $stream.Dispose(); $mint.Dispose(); $bg.Dispose(); $g.Dispose(); $bmp.Dispose()
}
$output = [System.IO.File]::Create($outPath)
$writer = [System.IO.BinaryWriter]::new($output)
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$images.Count)
$offset = 6 + 16 * $images.Count
foreach ($item in $images) {
    $dimension = if ($item[0] -eq 256) { 0 } else { $item[0] }
    $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
    $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([uint16]1); $writer.Write([uint16]32)
    $writer.Write([uint32]$item[1].Length); $writer.Write([uint32]$offset)
    $offset += $item[1].Length
}
foreach ($item in $images) { $writer.Write([byte[]]$item[1]) }
$writer.Dispose()
