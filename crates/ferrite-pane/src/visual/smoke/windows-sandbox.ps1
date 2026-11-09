# A test of effective Windows restrictions, independent of CEF settings/broker
# pointers. Microsoft documents IsTokenRestricted and TokenIntegrityLevel;
# Chromium documents restricted renderer tokens at Untrusted integrity:
# https://www.chromium.org/Home/chromium-security/articles/chrome-sandbox-diagnostics-for-windows/
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class FerriteSandboxToken {
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll")]
    static extern bool IsTokenRestricted(IntPtr token);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool GetTokenInformation(IntPtr token, int kind, IntPtr buffer, uint size, out uint required);
    [DllImport("advapi32.dll")]
    static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
    [DllImport("advapi32.dll")]
    static extern IntPtr GetSidSubAuthority(IntPtr sid, uint index);

    public sealed class Evidence {
        public bool Restricted;
        public uint Integrity;
    }
    public static Evidence Inspect(uint pid) {
        IntPtr process = OpenProcess(0x1000, false, pid); // QUERY_LIMITED_INFORMATION
        if (process == IntPtr.Zero) throw new Win32Exception();
        IntPtr token = IntPtr.Zero;
        IntPtr buffer = IntPtr.Zero;
        try {
            if (!OpenProcessToken(process, 8, out token)) throw new Win32Exception(); // TOKEN_QUERY
            uint size;
            GetTokenInformation(token, 25, IntPtr.Zero, 0, out size); // TokenIntegrityLevel
            if (size == 0) throw new Win32Exception();
            buffer = Marshal.AllocHGlobal(checked((int)size));
            if (!GetTokenInformation(token, 25, buffer, size, out size)) throw new Win32Exception();
            // TOKEN_MANDATORY_LABEL starts with SID_AND_ATTRIBUTES.Sid.
            IntPtr sid = Marshal.ReadIntPtr(buffer);
            byte count = Marshal.ReadByte(GetSidSubAuthorityCount(sid));
            if (count == 0) throw new InvalidOperationException("Integrity SID has no authority");
            uint rid = unchecked((uint)Marshal.ReadInt32(GetSidSubAuthority(sid, (uint)(count - 1))));
            return new Evidence { Restricted = IsTokenRestricted(token), Integrity = rid };
        } finally {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            if (token != IntPtr.Zero) CloseHandle(token);
            CloseHandle(process);
        }
    }
}
'@

$browserPid = [uint32]$env:FERRITE_SMOKE_BROWSER_PID
$browser = Get-CimInstance Win32_Process -Filter "ProcessId = $browserPid"
if (-not $browser) { throw "Ferrite browser process $browserPid not found" }
$hostToken = [FerriteSandboxToken]::Inspect($browserPid)
# Negative control: the app itself must not be mistaken for a locked-down renderer.
if ($hostToken.Restricted -and $hostToken.Integrity -le 0x1000) {
    throw "Browser negative control is already restricted at low integrity"
}
$renderers = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $browserPid" | Where-Object {
    $_.ExecutablePath -eq $browser.ExecutablePath -and $_.CommandLine -match '(?:^|\s)--type=renderer(?:\s|$)'
})
if ($renderers.Count -eq 0) { throw "No actual CEF renderer child of browser $browserPid found" }
foreach ($renderer in $renderers) {
    $token = [FerriteSandboxToken]::Inspect([uint32]$renderer.ProcessId)
    if (-not $token.Restricted -or $token.Integrity -gt 0x1000) {
        throw "Renderer $($renderer.ProcessId) lacks OS restrictions: restricted=$($token.Restricted), integrity=$($token.Integrity)"
    }
    Write-Output "PASS Windows OS sandbox: renderer=$($renderer.ProcessId) restricted=$($token.Restricted) integrity=$($token.Integrity); browser=$browserPid restricted=$($hostToken.Restricted) integrity=$($hostToken.Integrity)"
}
