param([string]$Png)
Add-Type -AssemblyName System.Drawing
$b = New-Object System.Drawing.Bitmap $Png
$colors=@{}
for ($y=40; $y -lt $b.Height-30; $y+=3) {
  for ($x=10; $x -lt $b.Width-10; $x+=3) {
    $c=$b.GetPixel($x,$y)
    $key = "{0},{1},{2}" -f ([int]($c.R/24)*24),([int]($c.G/24)*24),([int]($c.B/24)*24)
    if ($colors.ContainsKey($key)) { $colors[$key]++ } else { $colors[$key]=1 }
  }
}
$b.Dispose()
$colors.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 15 | ForEach-Object { "$($_.Key): $($_.Value)" }
