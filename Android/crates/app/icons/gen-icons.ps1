# 图标派生脚本：从新 logo（256x256）生成两端桌面图标与官网 favicon
# icon.ico 采用 PNG-in-ICO 容器（Vista+ 支持），一次携带 16/24/32/48/64/128/256 七档
Add-Type -AssemblyName System.Drawing

$logoPath = Join-Path $PSScriptRoot 'logo-src.png'
$outIco = Join-Path $PSScriptRoot 'icon.ico'
$outPng512 = Join-Path $PSScriptRoot 'icon.png'

$sizes = @(16, 24, 32, 48, 64, 128, 256)

function New-PngBytes([System.Drawing.Image]$src, [int]$size) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.DrawImage($src, 0, 0, $size, $size)
  $g.Dispose()
  $ms = New-Object System.IO.MemoryStream
  $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  return $ms.ToArray()
}

function Write-Ico([string]$path, [System.Drawing.Image]$src, [int[]]$sizeList) {
  # ArrayList 存数组引用，避免 PS 管道把 byte[] 拆散成单字节序列
  $blobList = New-Object System.Collections.ArrayList
  foreach ($s in $sizeList) { [void]$blobList.Add((New-PngBytes $src $s)) }
  $ms = New-Object System.IO.MemoryStream
  $bw = New-Object System.IO.BinaryWriter($ms)
  $bw.Write([UInt16]0)                      # reserved
  $bw.Write([UInt16]1)                      # type: icon
  $bw.Write([UInt16]$sizeList.Count)        # image count
  $offset = 6 + 16 * $sizeList.Count
  for ($i = 0; $i -lt $sizeList.Count; $i++) {
    $blob = [byte[]]$blobList[$i]
    $s = $sizeList[$i]
    $bw.Write([Byte]($(if ($s -ge 256) { 0 } else { $s })))  # width (0 = 256)
    $bw.Write([Byte]($(if ($s -ge 256) { 0 } else { $s })))  # height
    $bw.Write([Byte]0)                    # palette
    $bw.Write([Byte]0)                    # reserved
    $bw.Write([UInt16]1)                  # color planes
    $bw.Write([UInt16]32)                 # bits per pixel
    $bw.Write([UInt32]$blob.Length)       # data size
    $bw.Write([UInt32]$offset)            # data offset
    $offset += $blob.Length
  }
  for ($i = 0; $i -lt $sizeList.Count; $i++) { $bw.Write([byte[]]$blobList[$i]) }
  $bw.Flush()
  [System.IO.File]::WriteAllBytes($path, $ms.ToArray())
  $bw.Dispose(); $ms.Dispose()
}

function Write-Png([string]$path, [System.Drawing.Image]$src, [int]$size) {
  $bytes = New-PngBytes $src $size
  [System.IO.File]::WriteAllBytes($path, $bytes)
}

$img = [System.Drawing.Image]::FromFile($logoPath)
try {
  Write-Ico $outIco $img $sizes
  Write-Png $outPng512 $img 512
  Write-Output "OK icon.ico (7 sizes) + icon.png 512 -> $outIco"
} finally { $img.Dispose() }
