param([string]$Out)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap 600, 500
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen(1110, 270, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
