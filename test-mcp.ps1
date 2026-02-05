param(
    [string]$Tool = "project_list",
    [string]$ArgsJson = "{}"
)

$token = $env:FOLD_TOKEN
$body = @{
    jsonrpc = "2.0"
    id = 1
    method = "tools/call"
    params = @{
        name = $Tool
        arguments = ($ArgsJson | ConvertFrom-Json)
    }
} | ConvertTo-Json -Depth 10 -Compress

$result = Invoke-RestMethod -Uri "http://localhost:8765/mcp" -Method POST -ContentType "application/json" -Headers @{Authorization="Bearer $token"} -Body $body

if ($result.result.content) {
    $result.result.content[0].text
} else {
    $result | ConvertTo-Json -Depth 10
}
