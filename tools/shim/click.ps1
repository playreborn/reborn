# One left click at screen coordinates (full resolution). ADR 0044 allows
# clicks only in the official client's install and add-to-account dialogs.
param([int]$X, [int]$Y)
Add-Type -Name M -Namespace U -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, System.UIntPtr e);
'@
[U.M]::SetCursorPos($X, $Y) | Out-Null
Start-Sleep -Milliseconds 150
[U.M]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 60
[U.M]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
"clicked $X,$Y"
