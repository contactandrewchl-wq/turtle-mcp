# Instalador de Turtle para Windows — un solo comando, sin prerrequisitos.
#
#   irm https://raw.githubusercontent.com/contactandrewchl-wq/turtle-mcp/main/install.ps1 | iex
#
# Descarga el binario ya compilado del último GitHub Release y lo deja en el PATH.
# No requiere Rust ni compilador C: solo PowerShell, que Windows ya trae.
#
# Repo privado (durante las pruebas): define primero un token con permiso de lectura, p. ej.
#   $env:GITHUB_TOKEN = (gh auth token)
# Opcionales:  $env:TURTLE_VERSION = 'v0.1.0'   (por defecto, la última release)
[CmdletBinding()]
param(
    [string] $Version = $env:TURTLE_VERSION,
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'turtle\bin')
)

$ErrorActionPreference = 'Stop'
$repo = 'contactandrewchl-wq/turtle-mcp'

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Turtle requiere Windows de 64 bits.'
}
# ARM64 nativo queda pendiente; en equipos ARM corre por emulación x86-64.
$target = 'x86_64-pc-windows-msvc'
$asset = "turtle-$target.zip"

$token = if ($env:GITHUB_TOKEN) { $env:GITHUB_TOKEN } elseif ($env:GH_TOKEN) { $env:GH_TOKEN } else { $null }
$apiHeaders = @{ 'User-Agent' = 'turtle-installer'; 'Accept' = 'application/vnd.github+json' }
if ($token) { $apiHeaders['Authorization'] = "Bearer $token" }

$relUrl = if ($Version) {
    "https://api.github.com/repos/$repo/releases/tags/$Version"
} else {
    "https://api.github.com/repos/$repo/releases/latest"
}

Write-Host "Buscando la release de $repo..."
$rel = Invoke-RestMethod -Uri $relUrl -Headers $apiHeaders
$item = $rel.assets | Where-Object { $_.name -eq $asset } | Select-Object -First 1
if (-not $item) {
    throw "La release '$($rel.tag_name)' no incluye $asset."
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("turtle-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp $asset

$dlHeaders = @{ 'User-Agent' = 'turtle-installer'; 'Accept' = 'application/octet-stream' }
if ($token) { $dlHeaders['Authorization'] = "Bearer $token" }

Write-Host "Descargando turtle $($rel.tag_name) ($target)..."
Invoke-WebRequest -Uri $item.url -Headers $dlHeaders -OutFile $zip

Expand-Archive -Path $zip -DestinationPath $tmp -Force
Copy-Item -Path (Join-Path $tmp "turtle-$target\turtle.exe") -Destination (Join-Path $InstallDir 'turtle.exe') -Force
Remove-Item -Recurse -Force $tmp

# Agregar la carpeta al PATH del usuario si aún no está.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$enColeccion = if ($userPath) { $userPath -split ';' } else { @() }
if ($enColeccion -notcontains $InstallDir) {
    [Environment]::SetEnvironmentVariable('Path', "$InstallDir;$userPath", 'User')
    Write-Host "Se agregó $InstallDir al PATH del usuario (abre una terminal nueva para usarlo)."
}

Write-Host ''
Write-Host "Turtle instalado en $InstallDir."
Write-Host 'Verifica con:  turtle --version'
