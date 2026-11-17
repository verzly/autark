$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$assets = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets'
New-Item -ItemType Directory -Force -Path $assets | Out-Null

$size = 256
$bitmap = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$graphics.Clear([System.Drawing.Color]::Transparent)

$rect = New-Object System.Drawing.Rectangle(10, 10, 236, 236)
$radius = 52
$diameter = $radius * 2
$shape = New-Object System.Drawing.Drawing2D.GraphicsPath
$shape.AddArc($rect.X, $rect.Y, $diameter, $diameter, 180, 90)
$shape.AddArc($rect.Right - $diameter, $rect.Y, $diameter, $diameter, 270, 90)
$shape.AddArc($rect.Right - $diameter, $rect.Bottom - $diameter, $diameter, $diameter, 0, 90)
$shape.AddArc($rect.X, $rect.Bottom - $diameter, $diameter, $diameter, 90, 90)
$shape.CloseFigure()

$background = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 13, 13, 13))
$border = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 44, 44, 44), 3)
$graphics.FillPath($background, $shape)
$graphics.DrawPath($border, $shape)

$font = New-Object System.Drawing.Font('Segoe UI Semibold', 138, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
$format = New-Object System.Drawing.StringFormat
$format.Alignment = [System.Drawing.StringAlignment]::Center
$format.LineAlignment = [System.Drawing.StringAlignment]::Center
$ink = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 237, 237, 237))
$graphics.DrawString('A', $font, $ink, (New-Object System.Drawing.RectangleF(0, 4, $size, $size)), $format)

$pngPath = Join-Path $assets 'icon.png'
$bitmap.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

$png = [System.IO.File]::ReadAllBytes($pngPath)
$icoPath = Join-Path $assets 'icon.ico'
$stream = [System.IO.File]::Create($icoPath)
$writer = New-Object System.IO.BinaryWriter($stream)
$writer.Write([UInt16]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]1)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([Byte]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]32)
$writer.Write([UInt32]$png.Length)
$writer.Write([UInt32]22)
$writer.Write($png)
$writer.Flush()
$writer.Dispose()
$stream.Dispose()

Write-Output ("icon.png {0} bytes, icon.ico {1} bytes" -f $png.Length, (Get-Item $icoPath).Length)
