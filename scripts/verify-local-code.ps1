# Windows-local Claude Proxy Code tool-cycle smoke test.
# Safe by default: without -Run it makes NO network calls.
# With -Run it sends a small synthetic test to the local loopback proxy, which
# can incur provider token charges. Never sends local file contents or invokes
# a host shell/file write. Tool_result is a synthetic deterministic fixture.
[CmdletBinding()]
param(
    [string]$GatewayUrl = "http://127.0.0.1:18181",
    [string]$Model = "claude-sonnet-5-5",
    [switch]$Run
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$baseUri = [Uri]$GatewayUrl
if (-not $baseUri.IsLoopback -or $baseUri.Scheme -ne "http") {
    throw "This smoke test only connects to an HTTP loopback endpoint."
}
if ($baseUri.UserInfo -or $baseUri.Query -or $baseUri.Fragment) {
    throw "Do not include credentials, query strings or fragments in GatewayUrl."
}
if ([string]::IsNullOrWhiteSpace($Model)) {
    throw "The exact local Claude route model is required."
}
if (-not $Run) {
    Write-Host "DRY RUN — no request sent, no file accessed, no provider charge."
    Write-Host "To run: set CLAUDE_PROXY_GATEWAY_TOKEN in this PowerShell session and invoke this script with -Run."
    Write-Host "The test sends a synthetic Read tool request and an invented README.md tool_result."
    return
}

# The token comes from the *local proxy configuration*. Never print it.
$gatewayToken = [Environment]::GetEnvironmentVariable("CLAUDE_PROXY_GATEWAY_TOKEN")
if ([string]::IsNullOrWhiteSpace($gatewayToken)) {
    throw "Set CLAUDE_PROXY_GATEWAY_TOKEN in this session before running the smoke test."
}
$endpoint = $GatewayUrl.TrimEnd("/") + "/v1/messages"
$headers = @{
    "Authorization" = "Bearer $gatewayToken"
    "anthropic-version" = "2023-06-01"
    "Content-Type" = "application/json"
}
function Send-Message([object]$Payload) {
    $body = ConvertTo-Json -InputObject $Payload -Depth 32 -Compress
    # Never emit request body or auth token into terminal/log output.
    try {
        return Invoke-RestMethod -Uri $endpoint -Method Post -Headers $headers -Body $body -TimeoutSec 90
    }
    catch {
        throw "Local gateway request failed. Confirm that the proxy runs, the route is mapped, its token is correct, and the provider supports structured tools. No request details were logged by this script."
    }
}
$nonce = "SMOKE_" + [Guid]::NewGuid().ToString("N").Substring(0, 16)
$firstUserText = "Read README.md with the Read tool before answering. Do not invent file contents."
$request = @{
    model = $Model
    max_tokens = 512
    stream = $false
    tools = @(@{
        name = "Read"
        description = "Read a file from the current workspace"
        input_schema = @{
            type = "object"
            properties = @{ path = @{ type = "string" } }
            required = @("path")
        }
    })
    messages = @(@{ role = "user"; content = $firstUserText })
}
$reply = Send-Message $request
$toolCalls = @($reply.content | Where-Object { $_.type -eq "tool_use" })
if ($toolCalls.Count -ne 1 -or $toolCalls[0].name -ne "Read" -or
    [string]::IsNullOrWhiteSpace([string]$toolCalls[0].id)) {
    throw "FAIL: Code-capable model did not return one valid Read tool_use. Do not claim Code compatibility."
}
if ([string]$toolCalls[0].input.path -notmatch "(?i)README\.md") {
    throw "FAIL: The Read tool did not target README.md."
}
Write-Host "PASS: structured Read tool_use with stable ID/name/path."
# This is not an actual filesystem read. It tests the provider's ability to
# incorporate an approved client tool_result without hallucinating the input.
$syntheticRead = "# Synthetic QA fixture" + [Environment]::NewLine + "Validation marker: $nonce"
$second = @{
    model = $Model
    max_tokens = 512
    stream = $false
    messages = @(
        @{ role = "user"; content = $firstUserText },
        @{ role = "assistant"; content = @($reply.content) },
        @{ role = "user"; content = @(@{
            type = "tool_result"
            tool_use_id = [string]$toolCalls[0].id
            content = $syntheticRead
        }) }
    )
}
$final = Send-Message $second
$text = (@($final.content | Where-Object { $_.type -eq "text" } |
    ForEach-Object { [string]$_.text })) -join " "
if ($final.stop_reason -ne "end_turn" -or -not $text.Contains($nonce)) {
    throw "FAIL: The final answer did not faithfully include the synthetic tool result marker."
}
Write-Host "PASS: tool_result was incorporated into the final answer."
Write-Host "LOCAL PROXY TOOL-CYCLE SMOKE PASSED. This is not a Claude Desktop Code or Cowork UI test."
