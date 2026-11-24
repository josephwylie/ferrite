<#
.SYNOPSIS
Make the local WSL distro a Ferrite Machine reached over ssh, for live proofs.

.DESCRIPTION
`up` builds ferrited from this checkout inside WSL (a copy in the distro's own
filesystem, release, the Server binary only), installs it under a temporary
HOME there, and starts an unprivileged sshd on 127.0.0.1:<Port> that takes only
a key made for it and runs every session under that HOME. On Windows it writes
that key, a known_hosts, an ssh config and an ssh.exe shim that always passes
the config, so neither the operator's ~/.ssh, their ssh-agent nor their saved
Machines are ever read. It prints the environment an isolated Ferrite needs.

`add` saves the Machine `-Name` (target `ferrite-wsl`) in a settings.json, such
as an fv run's. `down` stops what `up` started, on both sides, and removes its
state; `-KeepBuild` keeps the WSL build for the next `up`.

.EXAMPLE
pwsh -File scripts/wsl-machine.ps1 up
$env:PATH = "$env:TEMP\ferrite-wsl-machine\bin;$env:PATH"
python skills/verify-ferrite/scripts/fv_windows.py launch tree
pwsh -File scripts/wsl-machine.ps1 add -Settings "$env:TEMP\fv-tree\settings.json"
$env:FV_RUN = 'tree'
python skills/verify-ferrite/scripts/fv_windows.py cli -- --machine wsl tree send <dir> /home/<you>/ferrite-wsl-machine/home/sent
pwsh -File scripts/wsl-machine.ps1 down
#>
param(
    [Parameter(Mandatory, Position = 0)][ValidateSet('up', 'add', 'down')][string]$Action,
    [string]$Distro = 'Ubuntu',
    [int]$Port = 2222,
    [string]$Name = 'wsl',
    [string]$Settings,
    [switch]$KeepBuild
)
$ErrorActionPreference = 'Stop'

$checkout = Split-Path $PSScriptRoot -Parent
$here = Join-Path $env:TEMP 'ferrite-wsl-machine'
$alias = 'ferrite-wsl'

# Every step runs as a script file, never a command line: wsl.exe hands a
# command line to a shell that would expand it again.
function Invoke-Wsl([string]$Step, [string]$Script, [string[]]$Arguments = @()) {
    New-Item -ItemType Directory -Force $here | Out-Null
    $file = Join-Path $here "$Step.sh"
    [IO.File]::WriteAllText($file, "set -eu`nroot=`"`$HOME/ferrite-wsl-machine`"`n" + $Script.Replace("`r`n", "`n"))
    wsl.exe -d $Distro --cd $checkout -- bash "$(Get-WslDir $here)/$Step.sh" @Arguments
    if ($LASTEXITCODE) { throw "${Step}: exit $LASTEXITCODE" }
}

function Get-WslDir([string]$Windows) {
    (wsl.exe -d $Distro --cd $Windows -- pwd).Trim()
}

function Find-OpenSsh([string]$Tool) {
    $found = Get-Command $Tool -CommandType Application -All |
        Where-Object { -not $_.Source.StartsWith($here) } | Select-Object -First 1
    if (-not $found) { throw "no $Tool on PATH" }
    $found.Source
}

function Up {
    New-Item -ItemType Directory -Force (Join-Path $here 'bin') | Out-Null

    $started = Get-Date
    Invoke-Wsl 'build' @'
src="$root/src"
mkdir -p "$src"
rsync -a --delete --exclude=/target --exclude=/.git ./ "$src/"
cd "$src"
if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi
cargo build --release --locked -p ferrite-server --bin ferrited
du -sh --exclude=target "$src" | sed 's/^/wsl disk, source: /'
du -sh "$src/target" | sed 's/^/wsl disk, build: /'
'@
    Write-Host "built in $([int]((Get-Date) - $started).TotalSeconds) s"

    $client = Join-Path $here 'client'
    if (-not (Test-Path $client)) {
        & (Find-OpenSsh 'ssh-keygen.exe') -q -t ed25519 -N '' -C 'ferrite-wsl-machine' -f $client
        if ($LASTEXITCODE) { throw "ssh-keygen: exit $LASTEXITCODE" }
    }

    Invoke-Wsl 'sshd-setup' @'
here="$1"
port="$2"
version="$("$root/src/target/release/ferrited" --version | cut -d' ' -f2)"
home="$root/home"
installed="$home/.ferrite/runtime/versions/$version/ferrited"
for proc in /proc/[0-9]*; do
    case "$(readlink "$proc/exe" 2>/dev/null)" in
        "$home"/*) kill "${proc#/proc/}" 2>/dev/null || true ;;
    esac
done
mkdir -p "$(dirname "$installed")" "$root/sshd"
cp "$root/src/target/release/ferrited" "$installed.new"
mv -f "$installed.new" "$installed"
cd "$root/sshd"
[ -f host ] || ssh-keygen -q -t ed25519 -N '' -f host
cp host.pub "$here/host.pub"
cp "$here/client.pub" authorized_keys
chmod 600 authorized_keys
cat > session <<EOF
#!/bin/sh
export HOME='$home'
cd "\$HOME"
exec /bin/sh -c "\$SSH_ORIGINAL_COMMAND"
EOF
chmod 700 session
cat > sshd_config <<EOF
Port $port
ListenAddress 127.0.0.1
HostKey $root/sshd/host
PidFile $root/sshd/sshd.pid
AuthorizedKeysFile $root/sshd/authorized_keys
StrictModes yes
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
PermitRootLogin no
AllowUsers $(id -un)
AllowAgentForwarding no
AllowTcpForwarding no
PermitTunnel no
X11Forwarding no
ForceCommand $root/sshd/session
LogLevel VERBOSE
EOF
/usr/sbin/sshd -t -f sshd_config
printf '%s\n' "$(id -un)" > "$here/user"
'@ -Arguments (Get-WslDir $here), $Port

    $hostKey = (Get-Content (Join-Path $here 'host.pub') -Raw).Trim().Split(' ')
    $known = Join-Path $here 'known_hosts'
    [IO.File]::WriteAllText($known, "[127.0.0.1]:$Port $($hostKey[0]) $($hostKey[1])`n")
    $user = (Get-Content (Join-Path $here 'user') -Raw).Trim()
    $config = Join-Path $here 'ssh_config'
    Set-Content $config @"
Host $alias
    HostName 127.0.0.1
    Port $Port
    User $user
    IdentityFile $client
    IdentitiesOnly yes
    IdentityAgent none
    UserKnownHostsFile $known
    GlobalKnownHostsFile $known
    StrictHostKeyChecking yes
    ConnectTimeout 5
"@

    $shim = Join-Path $here 'ssh-shim.rs'
    Set-Content $shim @"
use std::process::{exit, Command};

fn main() {
    let status = Command::new(r"$(Find-OpenSsh 'ssh.exe')")
        .arg("-F")
        .arg(r"$config")
        .args(std::env::args_os().skip(1))
        .status();
    exit(match status {
        Ok(status) => status.code().unwrap_or(255),
        Err(error) => {
            eprintln!("ssh shim: {error}");
            255
        }
    });
}
"@
    $ssh = Join-Path $here 'bin\ssh.exe'
    if (-not (Test-Path $ssh)) {
        rustc --edition 2021 -O -o $ssh $shim
        if ($LASTEXITCODE) { throw "rustc: exit $LASTEXITCODE" }
    }

    & $ssh -o BatchMode=yes $alias 'true' 2>$null
    if ($LASTEXITCODE) {
        # sshd stays in the foreground so this wsl.exe keeps the distro running.
        $sshd = Join-Path $here 'sshd.sh'
        [IO.File]::WriteAllText($sshd, "exec /usr/sbin/sshd -D -e -f `"`$HOME/ferrite-wsl-machine/sshd/sshd_config`" 2>>`"`$HOME/ferrite-wsl-machine/sshd/sshd.log`"`n")
        $process = Start-Process wsl.exe -ArgumentList '-d', $Distro, '--', 'bash', "$(Get-WslDir $here)/sshd.sh" -WindowStyle Hidden -PassThru
        Set-Content (Join-Path $here 'wsl.pid') $process.Id
    }
    $deadline = (Get-Date).AddSeconds(15)
    while ($LASTEXITCODE -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
        & $ssh -o BatchMode=yes $alias 'true' 2>$null
    }
    if ($LASTEXITCODE) { throw "sshd on 127.0.0.1:$Port did not take the key: see ~/ferrite-wsl-machine/sshd/sshd.log in $Distro" }

    Write-Host "Machine target $alias answers. For an isolated Ferrite (fv gives each run a home of its own), in this shell:"
    Write-Host "  `$env:PATH = `"$here\bin;`$env:PATH`""
    Write-Host "then save it: pwsh -File scripts/wsl-machine.ps1 add -Settings <settings.json>"
}

function Add {
    if (-not $Settings) { throw 'add needs -Settings <settings.json>' }
    $saved = if (Test-Path $Settings) { Get-Content $Settings -Raw | ConvertFrom-Json -AsHashtable } else { @{} }
    $machines = @($saved['machines'] | Where-Object { $_ -and $_['name'] -ne $Name })
    $saved['machines'] = $machines + @(@{ name = $Name; target = $alias; enabled = $true })
    $folder = Split-Path $Settings -Parent
    if ($folder) { New-Item -ItemType Directory -Force $folder | Out-Null }
    $saved | ConvertTo-Json -Depth 20 | Set-Content $Settings
    Write-Host "saved Machine $Name ($alias) in $Settings"
}

function Down {
    Invoke-Wsl 'down' @'
if [ -f "$root/sshd/sshd.pid" ]; then kill "$(cat "$root/sshd/sshd.pid")" 2>/dev/null || true; fi
for proc in /proc/[0-9]*; do
    case "$(readlink "$proc/exe" 2>/dev/null)" in
        "$root"/home/*) kill "${proc#/proc/}" 2>/dev/null || true ;;
    esac
done
rm -rf "$root/home" "$root/sshd"
if [ "${1:-}" != keep ]; then rm -rf "$root"; fi
'@ -Arguments $(if ($KeepBuild) { 'keep' } else { 'drop' })
    $pidFile = Join-Path $here 'wsl.pid'
    if (Test-Path $pidFile) {
        $process = Get-Process -Id (Get-Content $pidFile) -ErrorAction SilentlyContinue
        if ($process -and $process.ProcessName -eq 'wsl') { $process.WaitForExit(5000) | Out-Null }
    }
    Remove-Item -Recurse -Force $here
    Write-Host 'stopped and removed'
}

switch ($Action) {
    'up' { Up }
    'add' { Add }
    'down' { Down }
}
