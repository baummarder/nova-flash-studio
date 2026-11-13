$exe = "C:\Users\admin\Documents\Flash-Editor\dist\windows\NOVA-Flash-Studio-0.2.0-windows-x64-Setup.exe"
$v = & $exe --version
"V count=$($v.Count) first=$($v[0]) code=$LASTEXITCODE"
$s = & $exe --smoke-test
"S count=$($s.Count) code=$LASTEXITCODE"
$s | ForEach-Object { "LINE: $_" }
