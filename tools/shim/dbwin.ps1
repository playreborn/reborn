# Prints OutputDebugString messages of this session (the DBWIN protocol
# that DebugView uses), for $Seconds, filtered to lines that mention
# steamclient or S_API. Needs no administrator rights for the local session.
param([int]$Seconds = 120, [switch]$Launch, [string]$AppId = '2143030')
Add-Type -AssemblyName System.Core
$ready = New-Object System.Threading.EventWaitHandle($false, 'AutoReset', 'DBWIN_BUFFER_READY')
$data = New-Object System.Threading.EventWaitHandle($false, 'AutoReset', 'DBWIN_DATA_READY')
$map = [System.IO.MemoryMappedFiles.MemoryMappedFile]::CreateOrOpen('DBWIN_BUFFER', 4096)
$view = $map.CreateViewAccessor(0, 4096)
$bytes = New-Object byte[] 4092
$sw = [Diagnostics.Stopwatch]::StartNew()
if ($Launch) {
    Start-Process "steam://rungameid/$AppId"
    "{0:HH:mm:ss.fff} launched" -f (Get-Date)
}
while ($sw.Elapsed.TotalSeconds -lt $Seconds) {
    [void]$ready.Set()
    if ($data.WaitOne(500)) {
        $procId = $view.ReadUInt32(0)
        [void]$view.ReadArray(4, $bytes, 0, 4092)
        $end = [Array]::IndexOf($bytes, [byte]0)
        if ($end -lt 0) { $end = 4092 }
        $text = [Text.Encoding]::Default.GetString($bytes, 0, $end).Trim()
        if ($text -match 'steamclient|S_API|steam_api') {
            $text = $text -replace [regex]::Escape($env:USERNAME), '<user>'
            "{0:HH:mm:ss.fff} pid {1}: {2}" -f (Get-Date), $procId, $text
        }
    }
}
