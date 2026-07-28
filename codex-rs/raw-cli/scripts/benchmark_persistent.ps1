param(
    [Parameter(Mandatory = $true)]
    [string] $Exe,

    [string] $Model = 'gpt-5.4-mini',

    [string] $Prompt = 'hello',

    [ValidateRange(1, 100)]
    [int] $Runs = 10,

    [ValidateRange(10, 600)]
    [int] $TimeoutSeconds = 120
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Read-LineWithTimeout {
    param([System.IO.TextReader] $Reader)

    $task = $Reader.ReadLineAsync()
    return $task.WaitAsync([TimeSpan]::FromSeconds($TimeoutSeconds)).GetAwaiter().GetResult()
}

function Write-JsonLine {
    param(
        [System.IO.TextWriter] $Writer,
        [object] $Value
    )

    $Writer.WriteLine(($Value | ConvertTo-Json -Depth 30 -Compress))
    $Writer.Flush()
}

function Start-RawProcess {
    param(
        [string] $Mode,
        [string[]] $ExtraArguments = @(),
        [switch] $ReadStandardOutput
    )

    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $script:ExactExe
    $start.WorkingDirectory = (Get-Location).Path
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardInput = $ReadStandardOutput
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    [void] $start.ArgumentList.Add('--model')
    [void] $start.ArgumentList.Add($Model)
    [void] $start.ArgumentList.Add($Mode)
    foreach ($argument in $ExtraArguments) {
        [void] $start.ArgumentList.Add($argument)
    }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) {
        throw "Failed to start $Mode"
    }
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $stdoutTask = if ($ReadStandardOutput) {
        $null
    } else {
        $process.StandardOutput.ReadToEndAsync()
    }
    return [pscustomobject]@{
        Process = $process
        StderrTask = $stderrTask
        StdoutTask = $stdoutTask
        Mode = $Mode
    }
}

function Stop-RawProcess {
    param([object] $Context)

    if ($null -eq $Context) {
        return
    }
    $process = $Context.Process
    if ($Context.Mode -eq 'app-server' -and -not $process.HasExited) {
        $process.StandardInput.Close()
        [void] $process.WaitForExit(3000)
    }
    if (-not $process.HasExited) {
        $process.Kill($true)
        [void] $process.WaitForExit(5000)
    }
}

function Read-AppMessage {
    param([object] $Context)

    $line = Read-LineWithTimeout $Context.Process.StandardOutput
    if ($null -eq $line) {
        $stderr = $Context.StderrTask.GetAwaiter().GetResult()
        throw "app-server stdout ended unexpectedly. stderr: $stderr"
    }
    return $line | ConvertFrom-Json -Depth 30
}

function Initialize-AppServer {
    param([object] $Context)

    Write-JsonLine $Context.Process.StandardInput ([ordered]@{
        id = 1
        method = 'initialize'
        params = @{ clientInfo = @{ name = 'raw_benchmark'; version = '1.0.0' } }
    })
    do {
        $message = Read-AppMessage $Context
    } until ($message.PSObject.Properties['id'] -and $message.id -eq 1)
    if ($message.PSObject.Properties['error']) {
        throw "app-server initialize failed: $($message | ConvertTo-Json -Compress)"
    }

    Write-JsonLine $Context.Process.StandardInput ([ordered]@{ method = 'initialized' })
    Write-JsonLine $Context.Process.StandardInput ([ordered]@{
        id = 2
        method = 'thread/start'
        params = @{ model = $Model; ephemeral = $true }
    })
    do {
        $message = Read-AppMessage $Context
    } until ($message.PSObject.Properties['id'] -and $message.id -eq 2)
    if ($message.PSObject.Properties['error']) {
        throw "app-server thread/start failed: $($message | ConvertTo-Json -Compress)"
    }
    return [string] $message.result.thread.id
}

function Measure-AppTurn {
    param(
        [object] $Context,
        [string] $ThreadId,
        [int] $RequestId
    )

    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    Write-JsonLine $Context.Process.StandardInput ([ordered]@{
        id = $RequestId
        method = 'turn/start'
        params = @{
            threadId = $ThreadId
            input = @(@{ type = 'text'; text = $Prompt })
        }
    })

    $turnId = $null
    $ttft = $null
    $text = ''
    $inputTokens = $null
    while ($true) {
        $message = Read-AppMessage $Context
        if ($message.PSObject.Properties['id'] -and $message.id -eq $RequestId) {
            if ($message.PSObject.Properties['error']) {
                throw "app-server turn/start failed: $($message | ConvertTo-Json -Compress)"
            }
            $turnId = [string] $message.result.turn.id
            continue
        }
        if (-not $message.PSObject.Properties['method']) {
            continue
        }
        if ($message.method -eq 'item/agentMessage/delta') {
            if ($turnId -and $message.params.turnId -ne $turnId) {
                continue
            }
            $delta = [string] $message.params.delta
            if ($delta.Length -gt 0 -and $null -eq $ttft) {
                $ttft = $watch.Elapsed.TotalMilliseconds
            }
            $text += $delta
        } elseif ($message.method -eq 'thread/tokenUsage/updated') {
            if (-not $turnId -or $message.params.turnId -eq $turnId) {
                $inputTokens = [int64] $message.params.tokenUsage.last.inputTokens
            }
        } elseif ($message.method -eq 'turn/completed') {
            if ($turnId -and $message.params.turn.id -ne $turnId) {
                continue
            }
            if ($message.params.turn.status -ne 'completed') {
                throw "app-server turn failed: $($message | ConvertTo-Json -Depth 10 -Compress)"
            }
            $total = $watch.Elapsed.TotalMilliseconds
            break
        }
    }
    if ($null -eq $ttft -or [string]::IsNullOrEmpty($text) -or $null -eq $inputTokens) {
        throw 'app-server sample ended without text delta, output, or usage'
    }
    return [pscustomobject]@{
        Mode = 'app-server'
        TTFT = [double] $ttft
        Total = [double] $total
        InputTokens = [int64] $inputTokens
        Output = $text
    }
}

function Get-FreeLoopbackPort {
    $listener = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Loopback,
        0
    )
    $listener.Start()
    try {
        return ([System.Net.IPEndPoint] $listener.LocalEndpoint).Port
    } finally {
        $listener.Stop()
    }
}

function Wait-ApiReady {
    param(
        [System.Net.Http.HttpClient] $Client,
        [string] $BaseUrl,
        [object] $Context
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($Context.Process.HasExited) {
            $stderr = $Context.StderrTask.GetAwaiter().GetResult()
            throw "api-server exited during startup. stderr: $stderr"
        }
        try {
            $response = $Client.GetAsync("$BaseUrl/readyz").GetAwaiter().GetResult()
            if ($response.IsSuccessStatusCode) {
                $response.Dispose()
                return
            }
            $response.Dispose()
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    throw 'api-server readiness timed out'
}

function Measure-ApiTurn {
    param(
        [System.Net.Http.HttpClient] $Client,
        [string] $BaseUrl
    )

    $payload = [ordered]@{
        model = $Model
        input = $Prompt
        stream = $true
        parallel_tool_calls = $false
    } | ConvertTo-Json -Compress
    $request = [System.Net.Http.HttpRequestMessage]::new(
        [System.Net.Http.HttpMethod]::Post,
        "$BaseUrl/v1/responses"
    )
    $request.Content = [System.Net.Http.StringContent]::new(
        $payload,
        [System.Text.Encoding]::UTF8,
        'application/json'
    )
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $response = $Client.SendAsync(
        $request,
        [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
    ).GetAwaiter().GetResult()
    try {
        if (-not $response.IsSuccessStatusCode) {
            $body = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
            throw "api-server returned $([int] $response.StatusCode): $body"
        }
        $stream = $response.Content.ReadAsStream()
        $reader = [System.IO.StreamReader]::new($stream)
        try {
            $eventName = $null
            $dataLines = [System.Collections.Generic.List[string]]::new()
            $ttft = $null
            $text = ''
            $inputTokens = $null
            $completed = $false
            while (-not $completed) {
                $line = Read-LineWithTimeout $reader
                if ($null -eq $line) {
                    throw 'Responses SSE ended without response.completed'
                }
                if ($line.StartsWith('event:')) {
                    $eventName = $line.Substring(6).Trim()
                } elseif ($line.StartsWith('data:')) {
                    [void] $dataLines.Add($line.Substring(5).TrimStart())
                } elseif ($line.Length -eq 0 -and $dataLines.Count -gt 0) {
                    $data = [string]::Join("`n", $dataLines)
                    $dataLines.Clear()
                    if ($data -ne '[DONE]') {
                        $event = $data | ConvertFrom-Json -Depth 30
                        $kind = if ($eventName) { $eventName } else { [string] $event.type }
                        if ($kind -eq 'response.output_text.delta') {
                            $delta = [string] $event.delta
                            if ($delta.Length -gt 0 -and $null -eq $ttft) {
                                $ttft = $watch.Elapsed.TotalMilliseconds
                            }
                            $text += $delta
                        } elseif ($kind -eq 'response.failed') {
                            throw "Responses stream failed: $data"
                        } elseif ($kind -eq 'response.completed') {
                            $inputTokens = [int64] $event.response.usage.input_tokens
                            $completed = $true
                            $total = $watch.Elapsed.TotalMilliseconds
                        }
                    }
                    $eventName = $null
                }
            }
        } finally {
            $reader.Dispose()
        }
    } finally {
        $response.Dispose()
        $request.Dispose()
    }
    if ($null -eq $ttft -or [string]::IsNullOrEmpty($text) -or $null -eq $inputTokens) {
        throw 'api-server sample ended without text delta, output, or usage'
    }
    return [pscustomobject]@{
        Mode = 'api-server'
        TTFT = [double] $ttft
        Total = [double] $total
        InputTokens = [int64] $inputTokens
        Output = $text
    }
}

function Get-Percentile {
    param(
        [double[]] $Sorted,
        [double] $Percentile
    )

    $index = [Math]::Max(0, [Math]::Ceiling($Percentile * $Sorted.Count) - 1)
    return $Sorted[$index]
}

function Get-Summary {
    param(
        [object[]] $Samples,
        [string] $Property
    )

    [double[]] $values = @($Samples | ForEach-Object { [double] $_.$Property } | Sort-Object)
    $middle = [Math]::Floor($values.Count / 2)
    $median = if ($values.Count % 2 -eq 0) {
        ($values[$middle - 1] + $values[$middle]) / 2
    } else {
        $values[$middle]
    }
    return [pscustomobject]@{
        Median = $median
        P90 = Get-Percentile $values 0.90
        P95 = Get-Percentile $values 0.95
        Min = $values[0]
        Max = $values[-1]
        Mean = ($values | Measure-Object -Average).Average
    }
}

$script:ExactExe = (Resolve-Path -LiteralPath $Exe).Path
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:ExactExe).Hash
$handler = [System.Net.Http.SocketsHttpHandler]::new()
$handler.UseProxy = $false
$http = [System.Net.Http.HttpClient]::new($handler)
$http.Timeout = [TimeSpan]::FromSeconds($TimeoutSeconds)
$apiToken = [Environment]::GetEnvironmentVariable('CODEX_RAW_API_TOKEN')
if (-not [string]::IsNullOrWhiteSpace($apiToken)) {
    $http.DefaultRequestHeaders.Authorization =
        [System.Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer', $apiToken)
}
$app = $null
$api = $null
try {
    $app = Start-RawProcess -Mode 'app-server' -ReadStandardOutput
    $threadId = Initialize-AppServer $app

    $port = Get-FreeLoopbackPort
    $api = Start-RawProcess -Mode 'api-server' -ExtraArguments @(
        '--listen', "127.0.0.1:$port", '--max-concurrency', '1'
    )
    $baseUrl = "http://127.0.0.1:$port"
    Wait-ApiReady $http $baseUrl $api

    $nextId = 100
    [void] (Measure-AppTurn $app $threadId $nextId)
    [void] (Measure-ApiTurn $http $baseUrl)

    $appSamples = [System.Collections.Generic.List[object]]::new()
    $apiSamples = [System.Collections.Generic.List[object]]::new()
    for ($index = 0; $index -lt $Runs; $index++) {
        $nextId++
        if ($index % 2 -eq 0) {
            $appSamples.Add((Measure-AppTurn $app $threadId $nextId))
            $apiSamples.Add((Measure-ApiTurn $http $baseUrl))
        } else {
            $apiSamples.Add((Measure-ApiTurn $http $baseUrl))
            $appSamples.Add((Measure-AppTurn $app $threadId $nextId))
        }
    }

    $allTokenCounts = @($appSamples.InputTokens) + @($apiSamples.InputTokens)
    if (@($allTokenCounts | Sort-Object -Unique).Count -ne 1) {
        throw "Input token counts differ across samples: $($allTokenCounts -join ', ')"
    }

    $appTtft = Get-Summary $appSamples.ToArray() 'TTFT'
    $appTotal = Get-Summary $appSamples.ToArray() 'Total'
    $apiTtft = Get-Summary $apiSamples.ToArray() 'TTFT'
    $apiTotal = Get-Summary $apiSamples.ToArray() 'Total'
    $table = @(
        [pscustomobject]@{
            Mode = 'app-server'; Runs = $Runs
            TTFT_Median = $appTtft.Median; TTFT_P90 = $appTtft.P90; TTFT_P95 = $appTtft.P95
            Total_Median = $appTotal.Median; Total_P90 = $appTotal.P90; Total_P95 = $appTotal.P95
        },
        [pscustomobject]@{
            Mode = 'api-server'; Runs = $Runs
            TTFT_Median = $apiTtft.Median; TTFT_P90 = $apiTtft.P90; TTFT_P95 = $apiTtft.P95
            Total_Median = $apiTotal.Median; Total_P90 = $apiTotal.P90; Total_P95 = $apiTotal.P95
        }
    )

    Write-Output "Executable: $script:ExactExe"
    Write-Output "SHA256: $hash"
    Write-Output "Model: $Model"
    Write-Output "Prompt: $Prompt"
    Write-Output "Input tokens: $($allTokenCounts[0])"
    $table | Format-Table -AutoSize
    [pscustomobject]@{
        TTFT_Median_ApiMinusApp_ms = $apiTtft.Median - $appTtft.Median
        Total_Median_ApiMinusApp_ms = $apiTotal.Median - $appTotal.Median
        TTFT_AppOverApi_Ratio = $appTtft.Median / $apiTtft.Median
        Total_AppOverApi_Ratio = $appTotal.Median / $apiTotal.Median
    } | Format-List
} finally {
    Stop-RawProcess $api
    Stop-RawProcess $app
    $http.Dispose()
    $handler.Dispose()
}
