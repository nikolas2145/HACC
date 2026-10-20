$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

$compiler = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $compiler)) {
    throw '.NET Framework C# compiler (csc.exe) is missing.'
}

cargo test --features gpu
if ($LASTEXITCODE -ne 0) { throw 'GPU tests failed.' }
cargo build --release --features gpu
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }

New-Item -ItemType Directory -Force -Path dist,release | Out-Null
& $compiler /nologo /utf8output /target:winexe /optimize+ /out:dist\HaccGui.exe `
    /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.Core.dll `
    desktop\HaccGui.cs
if ($LASTEXITCODE -ne 0) { throw 'GUI build failed.' }

& $compiler /nologo /utf8output /target:winexe /optimize+ /out:release\HACC-Setup-1.0.0.exe `
    /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.Core.dll `
    /reference:Microsoft.CSharp.dll /resource:target\release\hacc.exe,Hacc.Cli `
    /resource:dist\HaccGui.exe,Hacc.Gui desktop\HaccSetup.cs
if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }

Copy-Item -LiteralPath target\release\hacc.exe -Destination release\hacc.exe -Force
Copy-Item -LiteralPath dist\HaccGui.exe -Destination release\HaccGui.exe -Force
Get-FileHash release\HACC-Setup-1.0.0.exe -Algorithm SHA256 |
    Select-Object Hash,Path | Format-List
