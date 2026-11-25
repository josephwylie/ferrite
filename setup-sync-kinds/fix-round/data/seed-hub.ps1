param([Parameter(Mandatory)][string]$Run)
$ErrorActionPreference = 'Stop'
$live = "$env:TEMP\fxp-live"
$root = "$env:TEMP\fv-$Run"
$h = "$root\home"
python "$live\seed_home.py" hub $h
$env:HOME = $h; $env:USERPROFILE = $h; $env:CODEX_HOME = "$h\.codex"
Remove-Item Env:CLAUDE_CONFIG_DIR, Env:FERRITE_SOCKET, Env:FERRITE_THREAD_ID, Env:FERRITE_BIN -ErrorAction SilentlyContinue
claude plugin marketplace add -- cursor/plugins 2>&1 | Select-Object -Last 1
claude plugin install pstack@cursor-plugins 2>&1 | Select-Object -Last 1
codex plugin marketplace upgrade -- cursor-plugins 2>&1 | Select-Object -Last 1
python "$live\hub_settings.py" $h
python "$live\snap.py" $h $root "$live\hub-before.json"
Set-Location $env:TEMP
wsl.exe -d Ubuntu --cd ~ -- bash /mnt/c/Users/josep/AppData/Local/Temp/fxp-live/snap-machine.sh machine-before
python "$live\old_base.py" $h "$live\machine-before.json" $root
