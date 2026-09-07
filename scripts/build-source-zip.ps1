$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$outputBase = Join-Path $projectRoot "dist\source"
$version = (Get-Content -Raw (Join-Path $projectRoot 'app\package.json') | ConvertFrom-Json).version
$sourceRoot = Join-Path $outputBase "OziiDPI-Source-$version"
$zipPath = Join-Path $outputBase "OziiDPI-Source-$version.zip"

function Copy-FilteredTree {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination,
        [string[]]$ExcludedDirectories = @(),
        [string[]]$ExcludedFiles = @()
    )

    $sourcePath = (Resolve-Path -LiteralPath $Source).Path
    Get-ChildItem -LiteralPath $sourcePath -Recurse -File -Force | ForEach-Object {
        $relative = $_.FullName.Substring($sourcePath.Length + 1)
        $segments = $relative -split '[\\/]'
        $skipDirectory = $segments | Where-Object { $ExcludedDirectories -contains $_ }
        $skipFile = $false
        foreach ($pattern in $ExcludedFiles) {
            if ($_.Name -like $pattern) { $skipFile = $true; break }
        }
        if (-not $skipDirectory -and -not $skipFile) {
            $target = Join-Path $Destination $relative
            $targetDirectory = Split-Path -Parent $target
            New-Item -ItemType Directory -Path $targetDirectory -Force | Out-Null
            Copy-Item -LiteralPath $_.FullName -Destination $target -Force
        }
    }
}

New-Item -ItemType Directory -Path $outputBase -Force | Out-Null
$expectedPrefix = [System.IO.Path]::GetFullPath($outputBase).TrimEnd('\') + '\'
$resolvedSourceRoot = [System.IO.Path]::GetFullPath($sourceRoot)
if (-not $resolvedSourceRoot.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Unsafe source staging path: $resolvedSourceRoot"
}
if (Test-Path -LiteralPath $sourceRoot) {
    Remove-Item -LiteralPath $sourceRoot -Recurse -Force
}
if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}
New-Item -ItemType Directory -Path $sourceRoot -Force | Out-Null

foreach ($file in @("README.md", ".gitignore", "DISCORD_ONLY_README.txt", "THIRD_PARTY_NOTICES.md", "SECURITY.md")) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination $sourceRoot
}

Copy-FilteredTree -Source (Join-Path $projectRoot "app") -Destination (Join-Path $sourceRoot "app") -ExcludedDirectories @("node_modules", "dist", "target")
Copy-FilteredTree -Source (Join-Path $projectRoot "chrome-extension") -Destination (Join-Path $sourceRoot "chrome-extension")
Copy-FilteredTree -Source (Join-Path $projectRoot "backend") -Destination (Join-Path $sourceRoot "backend") -ExcludedDirectories @("target")
Copy-FilteredTree -Source (Join-Path $projectRoot "engine") -Destination (Join-Path $sourceRoot "engine") -ExcludedDirectories @(".git", "bin", "SpoofDPI") -ExcludedFiles @("test_spoofdpi.exe")
Copy-FilteredTree -Source (Join-Path $projectRoot "installer") -Destination (Join-Path $sourceRoot "installer")
Copy-FilteredTree -Source (Join-Path $projectRoot "scripts") -Destination (Join-Path $sourceRoot "scripts")
Copy-FilteredTree -Source (Join-Path $projectRoot "docs") -Destination (Join-Path $sourceRoot "docs") -ExcludedDirectories @("runtime")
Copy-FilteredTree -Source (Join-Path $projectRoot ".github") -Destination (Join-Path $sourceRoot ".github")

Compress-Archive -LiteralPath $sourceRoot -DestinationPath $zipPath -CompressionLevel Optimal
$hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
Write-Host "OziiDPI source ZIP ready: $zipPath"
Write-Host "SHA256: $hash"
