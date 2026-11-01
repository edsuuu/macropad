# Compila o app e gera target\installer\Macropad-Setup.exe com o Inno Setup 6, na versão do Cargo.toml.
# Uso: .\tools\build-installer.ps1   (feche o Macropad antes: o .exe aberto não deixa compilar)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Push-Location $root
try {
    cargo build --release
    if ($LASTEXITCODE) { throw 'cargo build falhou' }
    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 não encontrado' }
    & $iscc /Q "/DVersion=$version" installer\macropad.iss
    if ($LASTEXITCODE) { throw 'o Inno Setup falhou' }
    "target\installer\Macropad-Setup.exe ($version)"
} finally {
    Pop-Location
}
