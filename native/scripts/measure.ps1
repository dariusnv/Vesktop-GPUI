param(
    [int]$ProcessId = 0,
    [ValidateRange(2, 600)][int]$Seconds = 30,
    [string]$OutputPath
)
$ErrorActionPreference = 'Stop'
if (-not $ProcessId) {
    $candidates = @(Get-Process -Name 'vesktop-gpui' -ErrorAction SilentlyContinue)
    if ($candidates.Count -ne 1) { throw 'Pass -ProcessId for exactly one running native client.' }
    $ProcessId = $candidates[0].Id
}
$initial = Get-Process -Id $ProcessId
$cpuStart = $initial.TotalProcessorTime.TotalSeconds
$wall = [Diagnostics.Stopwatch]::StartNew()
$samples = @()
for ($index = 0; $index -lt $Seconds; $index++) {
    Start-Sleep -Seconds 1
    $process = Get-Process -Id $ProcessId
    $samples += [pscustomobject]@{
        workingSetBytes = $process.WorkingSet64
        privateBytes = $process.PrivateMemorySize64
        handles = $process.HandleCount
    }
}
$wall.Stop()
$cpuSeconds = $process.TotalProcessorTime.TotalSeconds - $cpuStart
$result = [ordered]@{
    capturedAt = [DateTimeOffset]::Now.ToString('o')
    processId = $ProcessId
    executable = $process.Path
    durationSeconds = [Math]::Round($wall.Elapsed.TotalSeconds, 3)
    logicalProcessors = [Environment]::ProcessorCount
    cpuPercentOneCore = [Math]::Round(100 * $cpuSeconds / $wall.Elapsed.TotalSeconds, 3)
    cpuPercentMachine = [Math]::Round(100 * $cpuSeconds / $wall.Elapsed.TotalSeconds / [Environment]::ProcessorCount, 3)
    averageWorkingSetMiB = [Math]::Round(($samples | Measure-Object workingSetBytes -Average).Average / 1MB, 2)
    peakWorkingSetMiB = [Math]::Round(($samples | Measure-Object workingSetBytes -Maximum).Maximum / 1MB, 2)
    averagePrivateMiB = [Math]::Round(($samples | Measure-Object privateBytes -Average).Average / 1MB, 2)
    note = 'Single-process observation. Offline sample workload; no Electron baseline or frame-time measurement.'
}
$json = $result | ConvertTo-Json
if ($OutputPath) { Set-Content -LiteralPath $OutputPath -Value $json }
Write-Output $json
