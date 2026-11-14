param([string]$Swf, [string]$Tag)
$dir = "C:\Users\admin\Documents\Flash-Editor\samples\ruffle-captures"
New-Item -ItemType Directory -Force $dir | Out-Null
powershell -ExecutionPolicy Bypass -File "C:\Users\admin\Documents\Flash-Editor\scripts\capture-window.ps1" -Swf $Swf -OutPng "$dir\${Tag}-a.png" -WaitSec 3
powershell -ExecutionPolicy Bypass -File "C:\Users\admin\Documents\Flash-Editor\scripts\capture-window.ps1" -Swf $Swf -OutPng "$dir\${Tag}-b.png" -WaitSec 5
