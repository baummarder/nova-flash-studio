param([string]$A,[string]$B)
Add-Type -AssemblyName System.Drawing
$ba = New-Object System.Drawing.Bitmap $A
$bb = New-Object System.Drawing.Bitmap $B
$diff=0;$total=0;$sum=0
# compare centre stage region only (skip window chrome ~ top 40px and borders)
for ($y=60; $y -lt $ba.Height-20; $y+=3) {
  for ($x=20; $x -lt $ba.Width-20; $x+=3) {
    $ca=$ba.GetPixel($x,$y); $cb=$bb.GetPixel($x,$y); $total++
    $d=[math]::Abs($ca.R-$cb.R)+[math]::Abs($ca.G-$cb.G)+[math]::Abs($ca.B-$cb.B)
    $sum+=$d; if($d -gt 30){$diff++}
  }
}
$ba.Dispose();$bb.Dispose()
"diffPixels=$([math]::Round(100*$diff/$total,2))% meanDelta=$([math]::Round($sum/$total,2))"
