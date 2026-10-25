# Installs the x64 installer silently with Start at login, checks what it
# installed, uninstalls it, and checks it is gone. Also checks that the arm64
# installer refuses to run here. Run on an x64 Windows machine where
# saw-auto-clicker is not installed; CI's installer job runs it on a fresh
# runner.
param(
    # A directory holding saw-auto-clicker-setup.exe for x64, as build.yml
    # uploads it, and the arm64 one in an arm64 subdirectory.
    [Parameter(Mandatory)] [string] $Built
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$installDir = Join-Path $env:LOCALAPPDATA 'Programs\saw-auto-clicker'
$installedExe = Join-Path $installDir 'saw-auto-clicker.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runValue = 'saw-auto-clicker'
$temp = [IO.Path]::GetTempPath()
$logs = Join-Path $temp 'saw-installer-logs'
New-Item -ItemType Directory -Force $logs | Out-Null

function Fail([string] $message) {
    Get-ChildItem $logs -File | ForEach-Object {
        Write-Host "--- $($_.Name)"
        Get-Content $_.FullName | Write-Host
    }
    throw $message
}

# Waits up to a deadline, so a setup stuck on a message box fails the job
# instead of hanging it.
function Invoke-Setup([string] $exe, [string[]] $arguments) {
    $process = Start-Process $exe -ArgumentList $arguments -PassThru
    # Without a handle taken while it runs, ExitCode reads empty afterwards.
    $null = $process.Handle
    if (-not $process.WaitForExit(120000)) {
        $process.Kill($true)
        Fail "$exe $arguments did not finish within 2 minutes"
    }
    $process.ExitCode
}

function Get-RunCommand {
    (Get-ItemProperty $runKey -Name $runValue -ErrorAction SilentlyContinue)?.$runValue
}

function Wait-Until([scriptblock] $condition, [string] $what) {
    $deadline = (Get-Date).AddSeconds(30)
    while (-not (& $condition)) {
        if ((Get-Date) -gt $deadline) { Fail "after 30 s, still not true: $what" }
        Start-Sleep -Milliseconds 500
    }
}

$workspaceVersion = python -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])'
if ($LASTEXITCODE -ne 0) { throw 'could not read the workspace version from Cargo.toml' }

if (Test-Path $installDir) { throw "$installDir already exists; run this where saw-auto-clicker is not installed" }

Write-Host '== the arm64 installer refuses to run on x64'
$code = Invoke-Setup (Join-Path $Built 'arm64\saw-auto-clicker-setup.exe') @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$logs\arm64-setup.log`"")
Write-Host "exit code $code"
if ($code -eq 0) { Fail 'the arm64 installer exited 0 on x64' }
if (Test-Path $installDir) { Fail "the arm64 installer created $installDir on x64" }

Write-Host '== install silently with Start at login'
$code = Invoke-Setup (Join-Path $Built 'saw-auto-clicker-setup.exe') @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/TASKS=startatlogin', "/LOG=`"$logs\setup.log`"")
if ($code -ne 0) { Fail "setup exited $code" }
if (-not (Test-Path $installedExe)) { Fail "$installedExe is missing" }

Write-Host '== the installed exe reports the workspace version'
# Redirected, because a GUI-subsystem program has no console to print to.
$out = Join-Path $temp 'saw-version.txt'
$process = Start-Process $installedExe -ArgumentList '--version' -RedirectStandardOutput $out -NoNewWindow -PassThru -Wait
$reported = (Get-Content $out -Raw).Trim()
Write-Host "exit code $($process.ExitCode), stdout: $reported"
if ($process.ExitCode -ne 0 -or $reported -ne "saw-app $workspaceVersion") {
    Fail "expected exit code 0 and 'saw-app $workspaceVersion'"
}

Write-Host '== the Run value holds the quoted installed path'
$command = Get-RunCommand
Write-Host "Run value: $command"
if ($command -ne "`"$installedExe`"") { Fail "expected `"$installedExe`"" }

Write-Host '== uninstall silently'
$code = Invoke-Setup (Join-Path $installDir 'unins000.exe') @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$logs\uninstall.log`"")
if ($code -ne 0) { Fail "uninstall exited $code" }

# The uninstaller hands its work to a copy of itself in %TEMP% and can exit
# before that copy finishes: https://jrsoftware.org/ishelp/topic_uninstexitcodes.htm
Wait-Until { -not (Test-Path $installedExe) } "$installedExe is removed"
Wait-Until { $null -eq (Get-RunCommand) } 'the Run value is removed'
Write-Host 'installed, checked and uninstalled cleanly'
