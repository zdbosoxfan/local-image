$ErrorActionPreference='Stop'
$fixtureRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'fixtures'))+[IO.Path]::DirectorySeparatorChar
$dataRoot=[IO.Path]::GetFullPath('C:\Users\Owner\Documents\RapidRAW-AI-Connector\local-remove-data')
$sessionRoot=Join-Path $dataRoot 'sessions'
$collectionRoot=Join-Path $dataRoot 'collections'
$removedSessions=@(); $removedCollections=@()
foreach($file in (Get-ChildItem -LiteralPath $sessionRoot -Filter session.json -Recurse -File)) {
    $data=Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
    $ownedPath=$data.source_path
    if(!$ownedPath) {$ownedPath=$data.project_path}
    if(!$ownedPath) {continue}
    $source=[IO.Path]::GetFullPath($ownedPath)
    if(!$source.StartsWith($fixtureRoot,[StringComparison]::OrdinalIgnoreCase)) {continue}
    $target=[IO.Path]::GetFullPath($file.DirectoryName)
    if((Split-Path $target -Parent) -ne $sessionRoot -or (Split-Path $target -Leaf) -ne $data.id) {throw 'Unexpected test session target'}
    $removedSessions+=$data.id
    Remove-Item -LiteralPath $target -Recurse -Force
}
foreach($file in (Get-ChildItem -LiteralPath $collectionRoot -Filter '*.json' -File)) {
    $data=Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
    if(!$data.entries -or @($data.entries).Count -eq 0) {continue}
    $allFixtures=$true
    foreach($entry in $data.entries) {if(!$entry.path -or !([IO.Path]::GetFullPath($entry.path)).StartsWith($fixtureRoot,[StringComparison]::OrdinalIgnoreCase)) {$allFixtures=$false}}
    if(!$allFixtures) {continue}
    $target=[IO.Path]::GetFullPath($file.FullName)
    if((Split-Path $target -Parent) -ne $collectionRoot -or $file.BaseName -ne $data.id) {throw 'Unexpected test collection target'}
    $removedCollections+=$data.id
    Remove-Item -LiteralPath $target -Force
}
@{Sessions=$removedSessions;Collections=$removedCollections} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'test-session-cleanup.json') -Encoding UTF8
Write-Output ('Removed '+$removedSessions.Count+' test sessions and '+$removedCollections.Count+' test collections; user sessions untouched.')
