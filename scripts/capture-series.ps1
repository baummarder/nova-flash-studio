param([string]$Swf)
$dir = "C:\Users\admin\Documents\Flash-Editor\samples\ruffle-captures"
foreach ($t in 2,3,4,5,6) {
  powershell -ExecutionPolicy Bypass -File "C:\Users\admin\Documents\Flash-Editor\scripts\capture-window.ps1" -Swf $Swf -OutPng "$dir\mc-$t.png" -WaitSec $t | Out-Null
}
