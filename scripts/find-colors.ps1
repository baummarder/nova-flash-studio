param([string]$Png)
Add-Type -AssemblyName System.Drawing
$b = New-Object System.Drawing.Bitmap $Png
$hits=@{}
for ($y=0; $y -lt $b.Height; $y+=2) {
  for ($x=0; $x -lt $b.Width; $x+=2) {
    $c=$b.GetPixel($x,$y)
    # checker blue ~ (70,120,200) and light ~ (240,244,250)
    if ([math]::Abs($c.R-70) -lt 25 -and [math]::Abs($c.G-120) -lt 25 -and [math]::Abs($c.B-200) -lt 30) { $hits["blue"]++ }
    if ($c.R -gt 225 -and $c.G -gt 230 -and $c.B -gt 235) { $hits["light"]++ }
  }
}
$b.Dispose()
$hits.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }
