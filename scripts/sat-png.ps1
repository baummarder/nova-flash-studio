param([string]$Png)
Add-Type -AssemblyName System.Drawing
$b=New-Object System.Drawing.Bitmap $Png
$sat=0;$minx=9999;$maxx=0;$miny=9999;$maxy=0
for($y=0;$y -lt $b.Height;$y+=2){for($x=0;$x -lt $b.Width;$x+=2){
 $c=$b.GetPixel($x,$y)
 $mx=[math]::Max($c.R,[math]::Max($c.G,$c.B));$mn=[math]::Min($c.R,[math]::Min($c.G,$c.B))
 if($mx-$mn -gt 40){$sat++;if($x -lt $minx){$minx=$x};if($x -gt $maxx){$maxx=$x};if($y -lt $miny){$miny=$y};if($y -gt $maxy){$maxy=$y}}
}}
$b.Dispose()
"saturated=$sat bbox=$minx,$miny - $maxx,$maxy"
