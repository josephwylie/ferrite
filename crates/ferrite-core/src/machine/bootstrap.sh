set -eu
umask 077
version='@VERSION@'
root="$HOME/.ferrite/runtime"
binary="$root/versions/$version/ferrited"
stage=''
locked=''
service_log=''
service_pid=''
timer=''
fail() { printf '%s\n' "Ferrite setup: $*" >&2; exit 1; }
phase() { printf '%s\n' "FERRITE_SETUP $1" >&2; }
warning() { printf '%s\n' "FERRITE_WARNING $*" >&2; }
cleanup() {
    if [ -n "$timer" ]; then kill "$timer" 2>/dev/null || true; fi
    if [ -n "$service_pid" ]; then kill "$service_pid" 2>/dev/null || true; fi
    if [ -n "$service_log" ]; then rm -f "$service_log"; fi
    if [ -n "$stage" ]; then rm -rf "$stage"; fi
    if [ -n "$locked" ] && [ "$(readlink "$locked" 2>/dev/null || true)" = "$$" ]; then
        rm -f "$locked"
    fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
exact() { [ -x "$1" ] && [ "$("$1" --version 2>/dev/null)" = "ferrited $version" ]; }
phase inspecting
if exact "$binary"; then
    :
elif command -v ferrited >/dev/null 2>&1; then
    binary=$(command -v ferrited)
else
    os=$(uname -s)
    architecture=$(uname -m)
    case "$os/$architecture" in
        Linux/x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux/aarch64|Linux/arm64) target=aarch64-unknown-linux-gnu ;;
        Darwin/arm64|Darwin/aarch64) target=aarch64-apple-darwin ;;
        *) fail "Automatic installation is unavailable for $os/$architecture. Install a compatible ferrited on PATH, then retry." ;;
    esac
    command -v tar >/dev/null 2>&1 || fail 'Install tar on this machine, then retry.'
    if command -v sha256sum >/dev/null 2>&1; then
        digest() { sha256sum "$1" | awk '{print $1}'; }
    elif command -v shasum >/dev/null 2>&1; then
        digest() { shasum -a 256 "$1" | awk '{print $1}'; }
    else
        fail 'Install sha256sum or shasum on this machine, then retry.'
    fi
    if command -v curl >/dev/null 2>&1; then
        download() { curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 --connect-timeout 15 --max-time 90 --output "$2" "$1"; }
    elif command -v wget >/dev/null 2>&1; then
        download() { wget --https-only --timeout=30 --tries=1 -q -O "$2" "$1"; }
    else
        fail 'Install curl or wget on this machine, then retry.'
    fi
    mkdir -p "$root/versions"
    lock="$root/.setup-$version.lock"
    waited=0
    until ln -s "$$" "$lock" 2>/dev/null; do
        owner=$(readlink "$lock" 2>/dev/null || true)
        case "$owner" in
            ''|*[!0-9]*) fail "An invalid setup lock exists at $lock. Remove it after confirming no setup is running." ;;
        esac
        if ! kill -0 "$owner" 2>/dev/null; then
            if [ "$(readlink "$lock" 2>/dev/null || true)" = "$owner" ]; then rm -f "$lock"; fi
            continue
        fi
        waited=$((waited + 1))
        [ "$waited" -lt 30 ] || fail 'Another Ferrite installation is still running. Wait for it to finish, then retry.'
        sleep 1
    done
    locked=$lock
    if ! exact "$binary"; then
        [ ! -e "$root/versions/$version" ] || fail "The managed Ferrite $version installation is damaged. Move $root/versions/$version aside, then retry."
        stage=$(mktemp -d "$root/.install-$version.XXXXXXXX")
        asset="ferrited-$version-$target.tar.gz"
        release="https://github.com/josephwylie/ferrite/releases/download/v$version"
        phase installing
        download "$release/SHA256SUMS" "$stage/SHA256SUMS" || fail "Could not download checksums for Ferrite $version. Check connectivity and that release v$version publishes server assets."
        download "$release/$asset" "$stage/$asset" || fail "Could not download $asset. Check connectivity and that release v$version publishes this server asset."
        expected=$(awk -v asset="$asset" '$2 == asset { print $1 }' "$stage/SHA256SUMS")
        [ "${#expected}" -eq 64 ] || fail "The release has no unique SHA256 checksum for $asset."
        case "$expected" in *[!0-9a-fA-F]*) fail "The release checksum for $asset is invalid." ;; esac
        actual=$(digest "$stage/$asset")
        [ "$actual" = "$expected" ] || fail 'The downloaded server failed SHA256 verification. Retry the download.'
        mkdir "$stage/$version"
        tar -xzf "$stage/$asset" -C "$stage/$version" ferrited || fail 'The server archive does not contain ferrited.'
        [ -f "$stage/$version/ferrited" ] && [ ! -L "$stage/$version/ferrited" ] || fail 'The server archive must contain a regular ferrited executable.'
        chmod 700 "$stage/$version/ferrited"
        exact "$stage/$version/ferrited" || fail "The downloaded server cannot run as Ferrite $version on this machine. Linux requires glibc 2.35 or newer."
        mv "$stage/$version" "$root/versions/" || exact "$binary" || fail 'Could not publish the verified Ferrite installation.'
    fi
    cleanup
    stage=''
    locked=''
    if [ "$os" = Linux ]; then
        phase starting
        service_log=$(mktemp "$root/.service.XXXXXXXX")
        "$binary" service install </dev/null >"$service_log" 2>&1 &
        service_pid=$!
        (sleep 15; kill "$service_pid" 2>/dev/null || true) </dev/null >/dev/null 2>&1 &
        timer=$!
        if wait "$service_pid"; then
            while IFS= read -r line; do
                case "$line" in *linger*|*outside*) warning "$line" ;; esac
            done <"$service_log"
        else
            warning 'The user service is unavailable. Agents may stop after your last SSH session ends. Configure ferrited service install and loginctl enable-linger on this machine.'
        fi
        service_pid=''
        kill "$timer" 2>/dev/null || true
        wait "$timer" 2>/dev/null || true
        timer=''
        rm -f "$service_log"
        service_log=''
    fi
fi
phase verifying
exec "$binary" bridge
