param([string]$Png)
Add-Type -AssemblyName System.Drawing
$b = New-Object System.Drawing.Bitmap $Png
$colors = @{}
$dark = 0; $total = 0
for ($y=0; $y -lt $b.Height; $y+=4) {
  for ($x=0; $x -lt $b.Width; $x+=4) {
    $c = $b.GetPixel($x,$y); $total++
    $key = "$($c.R/32),$($c.G/32),$($c.B/32)"
    $colors[$key] = ($colors[$key] + 1)
    if ($c.R -lt 20 -and $c.G -lt 20 -and $c.B -lt 20) { $dark++ }
  }
}
$b.Dispose()
"size=$($b.Width)x$($b.Height) sampled=$total uniqueBuckets=$($colors.Count) darkRatio=$([math]::Round($dark/$total,3))"
