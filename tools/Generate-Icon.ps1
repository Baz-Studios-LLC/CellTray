$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$iconPath = Join-Path $PSScriptRoot '..\assets\icons\celltray.ico'
$images = @()
foreach ($size in @(16, 24, 32, 48, 64, 128, 256)) {
    $bitmap = [System.Drawing.Bitmap]::new($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $scale = $size / 24.0
    $pen = [System.Drawing.Pen]::new([System.Drawing.Color]::White, [single](2 * $scale))
    $graphics.DrawEllipse($pen, [single](3*$scale), [single](3*$scale), [single](18*$scale), [single](18*$scale))
    foreach ($circle in @(@(10,11,3), @(16,9,1.2), @(15,16,1.2))) {
        $graphics.FillEllipse([System.Drawing.Brushes]::White, [single](($circle[0]-$circle[2])*$scale), [single](($circle[1]-$circle[2])*$scale), [single](2*$circle[2]*$scale), [single](2*$circle[2]*$scale))
    }
    $stream = [System.IO.MemoryStream]::new()
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $images += ,@($size, $stream.ToArray())
    $stream.Dispose(); $pen.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
}
$output = [System.IO.File]::Create($iconPath)
$writer = [System.IO.BinaryWriter]::new($output)
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$images.Count)
$offset = 6 + 16*$images.Count
foreach ($entry in $images) {
    $dimension = if ($entry[0] -eq 256) { 0 } else { $entry[0] }
    $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
    $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([uint16]1); $writer.Write([uint16]32)
    $writer.Write([uint32]$entry[1].Length); $writer.Write([uint32]$offset)
    $offset += $entry[1].Length
}
foreach ($entry in $images) { $writer.Write([byte[]]$entry[1]) }
$writer.Dispose(); $output.Dispose()
