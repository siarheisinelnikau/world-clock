param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class IconResourceProof {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")] public static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll")] public static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")] public static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")] public static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr LoadImage(IntPtr module, IntPtr name, uint type, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr icon);
    [DllImport("user32.dll")] public static extern bool DrawIconEx(IntPtr dc, int x, int y, IntPtr icon, int width, int height, uint step, IntPtr brush, uint flags);
}
'@
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$outputPath = [IO.Path]::GetFullPath($Output)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($outputPath)) | Out-Null
$module = [IconResourceProof]::LoadLibraryEx($executablePath, [IntPtr]::Zero, 2)
if ($module -eq [IntPtr]::Zero) { throw 'Cannot load executable icon resources.' }
try {
    $resource = [IconResourceProof]::FindResource($module, [IntPtr]1, [IntPtr]14)
    if ($resource -eq [IntPtr]::Zero) { throw 'Missing RT_GROUP_ICON 1.' }
    $length = [IconResourceProof]::SizeofResource($module, $resource)
    $group = [byte[]]::new($length)
    $loaded = [IconResourceProof]::LoadResource($module, $resource)
    [Runtime.InteropServices.Marshal]::Copy([IconResourceProof]::LockResource($loaded), $group, 0, $group.Length)
    $count = [BitConverter]::ToUInt16($group, 4)
    $sizes = @()
    for ($entry = 0; $entry -lt $count; $entry++) {
        $size = [int]$group[6 + 14 * $entry]
        if ($size -eq 0) { $size = 256 }
        $sizes += $size
    }
    foreach ($required in @(16,20,24,32,48,64,128,256)) {
        if ($sizes -notcontains $required) { throw "Missing native icon size $required." }
    }
    $proof = [Drawing.Bitmap]::new(640, 192)
    $graphics = [Drawing.Graphics]::FromImage($proof)
    try {
        $graphics.Clear([Drawing.Color]::White)
        $dark = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(24,28,36))
        try { $graphics.FillRectangle($dark, 0, 96, 640, 96) } finally { $dark.Dispose() }
        $font = [Drawing.Font]::new('Segoe UI', 10)
        $details = @()
        try {
            $column = 0
            foreach ($size in @(16,20,24,32,48,64,128,256)) {
                $icon = [IconResourceProof]::LoadImage($module,[IntPtr]1,1,$size,$size,0)
                if ($icon -eq [IntPtr]::Zero) { throw "Cannot load native icon at $size pixels." }
                try {
                    $bitmap = [Drawing.Icon]::FromHandle($icon).ToBitmap()
                    try {
                        $opaque = 0; $left = $size; $top = $size; $right = -1; $bottom = -1
                        for ($y = 0; $y -lt $bitmap.Height; $y++) {
                            for ($x = 0; $x -lt $bitmap.Width; $x++) {
                                if ($bitmap.GetPixel($x,$y).A -gt 16) {
                                    $opaque++; $left = [Math]::Min($left,$x); $top = [Math]::Min($top,$y)
                                    $right = [Math]::Max($right,$x); $bottom = [Math]::Max($bottom,$y)
                                }
                            }
                        }
                        if ($opaque -eq 0) { throw "Blank icon at $size pixels." }
                        $details += [pscustomobject]@{size=$size; alphaPixels=$opaque; alphaBounds=@($left,$top,$right,$bottom)}
                    } finally { $bitmap.Dispose() }
                    if ($size -gt 64) { continue }
                    foreach ($row in @(0,1)) {
                        $textBrush = if ($row -eq 0) { [Drawing.Brushes]::Black } else { [Drawing.Brushes]::White }
                        $graphics.DrawString("$size px", $font, $textBrush, 8 + 104*$column, 4+96*$row)
                        $dc = $graphics.GetHdc()
                        try {
                            if (-not [IconResourceProof]::DrawIconEx($dc, 8+104*$column, 27+96*$row, $icon, $size, $size, 0, [IntPtr]::Zero, 3)) { throw 'DrawIconEx failed.' }
                        } finally { $graphics.ReleaseHdc($dc) }
                    }
                } finally { [IconResourceProof]::DestroyIcon($icon) | Out-Null }
                $column++
            }
        } finally { $font.Dispose() }
        $proof.Save($outputPath, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $proof.Dispose() }
    [pscustomobject]@{executable=$executablePath;sha256=(Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant();resourceSizes=$sizes;nativeLoads=$details;proof=$outputPath;scope='Native embedded-resource proof; not an OS tray screenshot.'} | ConvertTo-Json -Depth 5
} finally { [IconResourceProof]::FreeLibrary($module) | Out-Null }
