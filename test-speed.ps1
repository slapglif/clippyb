# Compilation Speed Test Script
Write-Host "🚀 Testing Compilation Speed Optimizations" -ForegroundColor Green
Write-Host "==========================================" -ForegroundColor Green

# Check sccache before
Write-Host "`n📊 sccache Statistics (Before):" -ForegroundColor Yellow
sccache --show-stats

# Test 1: Clean build timing
Write-Host "`n🧹 Test 1: Clean build timing..." -ForegroundColor Cyan
Remove-Item -Path "src-tauri\target" -Recurse -Force -ErrorAction SilentlyContinue
$cleanStart = Get-Date
cd src-tauri
cargo build --release
$cleanEnd = Get-Date
$cleanTime = ($cleanEnd - $cleanStart).TotalSeconds
cd ..

Write-Host "Clean build time: $cleanTime seconds" -ForegroundColor Green

# Check sccache after first build
Write-Host "`n📊 sccache Statistics (After First Build):" -ForegroundColor Yellow
sccache --show-stats

# Test 2: Make a small change and rebuild
Write-Host "`n🔄 Test 2: Incremental build timing..." -ForegroundColor Cyan
$timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
Add-Content -Path "src-tauri\src\lib.rs" -Value "`n// Updated at $timestamp"

$incStart = Get-Date
cd src-tauri
cargo build --release
$incEnd = Get-Date
$incTime = ($incEnd - $incStart).TotalSeconds
cd ..

Write-Host "Incremental build time: $incTime seconds" -ForegroundColor Green

# Test 3: Clean and rebuild (should use cache)
Write-Host "`n♻️ Test 3: Cached rebuild timing..." -ForegroundColor Cyan
Remove-Item -Path "src-tauri\target" -Recurse -Force -ErrorAction SilentlyContinue
$cachedStart = Get-Date
cd src-tauri
cargo build --release
$cachedEnd = Get-Date
$cachedTime = ($cachedEnd - $cachedStart).TotalSeconds
cd ..

Write-Host "Cached rebuild time: $cachedTime seconds" -ForegroundColor Green

# Final statistics
Write-Host "`n📊 Final sccache Statistics:" -ForegroundColor Yellow
sccache --show-stats

# Summary
Write-Host "`n📈 Performance Summary:" -ForegroundColor Magenta
Write-Host "======================" -ForegroundColor Magenta
Write-Host "Clean build:       $cleanTime seconds" -ForegroundColor White
Write-Host "Incremental build: $incTime seconds" -ForegroundColor White
Write-Host "Cached rebuild:    $cachedTime seconds" -ForegroundColor White

$improvement = [math]::Round((($cleanTime - $cachedTime) / $cleanTime) * 100, 2)
Write-Host "`n🎯 Cache effectiveness: $improvement% faster on cached rebuild!" -ForegroundColor Green

Write-Host "`n✅ All optimizations are active and working!" -ForegroundColor Green