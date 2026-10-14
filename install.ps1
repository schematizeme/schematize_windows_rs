<#
.SYNOPSIS
  Instala o schematize no Windows (market + overdev, com as janelas) sem precisar de administrador.

.DESCRIPTION
  Baixa os executaveis do ultimo release de schematizeme/schematize_windows_rs para
  %USERPROFILE%\.cargo\bin, confere o SHA-256 (quando o release publica o SHA256SUMS), testa
  `--version` de cada um e poe a pasta no PATH do USUARIO (HKCU, idempotente). Nada de admin.
  Compativel com Windows PowerShell 5.1 e PowerShell 7.

  Uso (uma linha):
      irm https://raw.githubusercontent.com/schematizeme/schematize_windows_rs/main/install.ps1 | iex

  Com o kit completo (instala tambem as linguagens e ferramentas via `schematize-market install --all -y`):
      & ([scriptblock]::Create((irm https://raw.githubusercontent.com/schematizeme/schematize_windows_rs/main/install.ps1))) -Completo
  ou, no formato de uma linha:
      $env:SCHEMATIZE_COMPLETO = '1'; irm https://raw.githubusercontent.com/schematizeme/schematize_windows_rs/main/install.ps1 | iex

.PARAMETER Completo
  Depois de instalar, roda `schematize-market install --all -y`.

.PARAMETER Versao
  Tag do release (ex.: v0.1.0). Padrao: o ultimo.

.PARAMETER Destino
  Pasta de instalacao. Padrao: %USERPROFILE%\.cargo\bin.

.PARAMETER BaseUrl
  Sobrescreve a URL base dos downloads (usado pelo CI para testar um release especifico).

.PARAMETER SemPath
  Nao altera o PATH do usuario.
#>
[CmdletBinding()]
param(
    [switch]$Completo,
    [string]$Versao = 'latest',
    [string]$Destino = '',
    [string]$BaseUrl = '',
    [switch]$SemPath
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch { }

$Repo = 'schematizeme/schematize_windows_rs'
# CLIs obrigatorios; janelas opcionais (uma janela que falha nao derruba a instalacao).
$Obrigatorios = @('schematize-market', 'schematize-overdev')
$Janelas = @('schematize-market-gui', 'schematize-overdev-gui')

<# Mensagem de progresso. #>
function Say([string]$Msg) { Write-Host "-> $Msg" }

<# Falha com mensagem acionavel (sem culpar o usuario). #>
function Fail([string]$Msg) {
    Write-Host "erro: $Msg" -ForegroundColor Red
    throw $Msg
}

<# URL de download de um asset do release escolhido. #>
function Get-AssetUrl([string]$Nome) {
    if ($BaseUrl -ne '') { return ($BaseUrl.TrimEnd('/') + '/' + $Nome) }
    if ($Versao -eq 'latest') { return "https://github.com/$Repo/releases/latest/download/$Nome" }
    return "https://github.com/$Repo/releases/download/$Versao/$Nome"
}

<# Baixa $Url em $Dest via arquivo .part (nunca deixa arquivo pela metade). #>
function Get-Arquivo([string]$Url, [string]$Dest) {
    $part = "$Dest.part"
    if (Test-Path -LiteralPath $part) { Remove-Item -LiteralPath $part -Force }
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $part -Headers @{ 'User-Agent' = 'schematize-install' }
    } catch {
        if (Test-Path -LiteralPath $part) { Remove-Item -LiteralPath $part -Force }
        throw
    }
    Move-Item -LiteralPath $part -Destination $Dest -Force
}

<# SHA-256 esperado de $Nome segundo o SHA256SUMS ($null se o release nao publica). #>
function Get-HashEsperado([string]$Nome) {
    if ($null -eq $script:Sums) { return $null }
    foreach ($linha in $script:Sums) {
        if ($linha -match '^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$' -and $Matches[2] -eq $Nome) { return $Matches[1].ToLower() }
    }
    return $null
}

<# Poe $Novo em $Alvo, aposentando um .exe em uso como .old (o Windows nao deixa sobrescrever). #>
function Install-Exe([string]$Novo, [string]$Alvo) {
    if (Test-Path -LiteralPath $Alvo) {
        $old = [IO.Path]::ChangeExtension($Alvo, '.old')
        if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue }
        if (Test-Path -LiteralPath $old) { $old = $old + (Get-Random) }
        Move-Item -LiteralPath $Alvo -Destination $old -Force
    }
    Move-Item -LiteralPath $Novo -Destination $Alvo -Force
    $velhos = Get-ChildItem -LiteralPath (Split-Path -Parent $Alvo) -Filter 'schematize*.old*' -ErrorAction SilentlyContinue
    foreach ($v in $velhos) { Remove-Item -LiteralPath $v.FullName -Force -ErrorAction SilentlyContinue }
}

<# Garante $Dir no PATH do USUARIO (registro, REG_EXPAND_SZ, idempotente) e na sessao atual. #>
function Add-UserPath([string]$Dir) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $atual = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $alvo = $Dir.TrimEnd('\').ToLower()
        $ja = $false
        foreach ($p in $atual.Split(';')) {
            $q = [Environment]::ExpandEnvironmentVariables($p.Trim().Trim('"')).TrimEnd('\').ToLower()
            if ($q -ne '' -and $q -eq $alvo) { $ja = $true; break }
        }
        if (-not $ja) {
            $novo = if ($atual.Trim() -eq '') { $Dir } else { $atual.TrimEnd(';') + ';' + $Dir }
            $key.SetValue('Path', $novo, [Microsoft.Win32.RegistryValueKind]::ExpandString)
            Say "PATH do usuario atualizado (+ $Dir)"
            try {
                Add-Type -Namespace Schematize -Name Env -MemberDefinition '[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'
                $r = [UIntPtr]::Zero
                [void][Schematize.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$r)
            } catch {
                Write-Warning "PATH gravado, mas nao consegui avisar o sistema: $($_.Exception.Message). Abra um terminal novo."
            }
        }
    } finally { $key.Close() }
    $sessao = ($env:Path -split ';') | ForEach-Object { $_.TrimEnd('\').ToLower() }
    if ($sessao -notcontains $Dir.TrimEnd('\').ToLower()) { $env:Path = $env:Path.TrimEnd(';') + ';' + $Dir }
}

# ----------------------------------------------------------------------------------------------
if (-not [Environment]::Is64BitOperatingSystem) { Fail 'o schematize precisa de Windows de 64 bits.' }
if ($Destino -eq '') { $Destino = Join-Path $env:USERPROFILE '.cargo\bin' }
if (-not $Completo -and $env:SCHEMATIZE_COMPLETO -eq '1') { $Completo = $true }

New-Item -ItemType Directory -Force -Path $Destino | Out-Null
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('schematize-install-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    $script:Sums = $null
    try {
        Get-Arquivo (Get-AssetUrl 'SHA256SUMS') (Join-Path $tmp 'SHA256SUMS')
        $script:Sums = Get-Content -LiteralPath (Join-Path $tmp 'SHA256SUMS')
    } catch {
        Write-Warning 'este release nao publica SHA256SUMS; seguindo sem conferir o hash.'
    }

    foreach ($base in ($Obrigatorios + $Janelas)) {
        $asset = "$base-windows-x86_64.exe"
        $obrigatorio = $Obrigatorios -contains $base
        $baixado = Join-Path $tmp $asset
        Say "baixando $asset"
        try {
            Get-Arquivo (Get-AssetUrl $asset) $baixado
        } catch {
            if ($obrigatorio) { Fail "nao consegui baixar $asset ($($_.Exception.Message)). Veja a conexao e tente de novo." }
            Write-Warning "janela $asset indisponivel neste release; seguindo sem ela."
            continue
        }
        $esperado = Get-HashEsperado $asset
        if ($null -ne $esperado) {
            $real = (Get-FileHash -LiteralPath $baixado -Algorithm SHA256).Hash.ToLower()
            if ($real -ne $esperado) { Fail "SHA-256 de $asset nao confere (esperado $esperado, veio $real). Nada foi instalado." }
        } elseif ($null -ne $script:Sums) {
            Fail "$asset nao consta no SHA256SUMS; recuso instalar sem conferir."
        }
        Install-Exe $baixado (Join-Path $Destino "$base.exe")
    }

    foreach ($base in $Obrigatorios) {
        $exe = Join-Path $Destino "$base.exe"
        # stderr de um nativo vira ErrorRecord no PS 5.1 com 'Stop': relaxa so durante a chamada.
        $prev = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $v = & $exe --version 2>&1
        $codigo = $LASTEXITCODE
        $ErrorActionPreference = $prev
        if ($codigo -ne 0) { Fail "$base.exe nao executa (--version saiu com $codigo): $v" }
        Say "$v"
    }

    if (-not $SemPath) { Add-UserPath $Destino }

    Write-Host ''
    Write-Host 'Pronto. Abra um NOVO terminal e rode:  schematize-market' -ForegroundColor Green

    if ($Completo) {
        Say 'kit completo: schematize-market install --all -y'
        & (Join-Path $Destino 'schematize-market.exe') install --all -y
        if ($LASTEXITCODE -ne 0) { Fail "schematize-market install --all saiu com $LASTEXITCODE." }
    }
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
