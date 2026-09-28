# Install Plan My Cabinet from the latest GitHub release on Windows.
#
#   irm https://raw.githubusercontent.com/killertux/plan-my-cabinet/master/install.ps1 | iex
#
# Set $env:PMC_VERSION (e.g. "v0.1.0") to pick a release.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'killertux/plan-my-cabinet'
$name = 'plan-my-cabinet'

if (-not [Environment]::Is64BitOperatingSystem) { throw 'Only 64-bit Windows is supported.' }

$version = $env:PMC_VERSION
if (-not $version) {
    $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
}
$asset = "$name-$($version.TrimStart('v'))-windows-x86_64.zip"
$base = "https://github.com/$repo/releases/download/$version"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null

try {
    Write-Host "Downloading Plan My Cabinet $version..."
    $zip = Join-Path $tmp $asset
    Invoke-WebRequest "$base/$asset" -OutFile $zip

    $sums = $null
    try { $sums = Invoke-WebRequest "$base/SHA256SUMS" -UseBasicParsing } catch {
        Write-Warning 'No SHA256SUMS in the release; skipping checksum.'
    }
    if ($sums) {
        $text = if ($sums.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($sums.Content) } else { $sums.Content }
        $line = $text -split "`n" | Where-Object { $_ -match "\s\*?$([regex]::Escape($asset))\s*$" }
        $expected = ($line -split '\s+')[0]
        $actual = (Get-FileHash $zip -Algorithm SHA256).Hash
        if (-not $expected -or $expected -ne $actual.ToLower()) { throw "Checksum mismatch for $asset" }
    }

    Expand-Archive $zip -DestinationPath $tmp
    $dest = Join-Path $env:LOCALAPPDATA 'Programs\PlanMyCabinet'
    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    Move-Item (Join-Path $tmp "$name-$($version.TrimStart('v'))-windows-x86_64") $dest

    $exe = Join-Path $dest "$name.exe"
    $menu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Plan My Cabinet.lnk'
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($menu)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $dest
    $link.Save()

    Write-Host "Installed $exe"
    Write-Host 'Start it from the Start menu: Plan My Cabinet'
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
