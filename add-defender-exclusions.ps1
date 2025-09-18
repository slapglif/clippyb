# Windows Defender Exclusions for Development Environment
# Run this script as Administrator

Write-Host "Adding Windows Defender exclusions for development environment..." -ForegroundColor Green

# Check if running as administrator
if (-NOT ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator"))
{
    Write-Host "ERROR: This script must be run as Administrator!" -ForegroundColor Red
    Write-Host "Right-click PowerShell and select 'Run as Administrator'" -ForegroundColor Yellow
    pause
    exit 1
}

try {
    # Add directory exclusions
    Write-Host "Adding path exclusions..." -ForegroundColor Yellow
    Add-MpPreference -ExclusionPath "C:\Users\MichaelBrown\work"
    Add-MpPreference -ExclusionPath "C:\Users\MichaelBrown\.cargo"
    Add-MpPreference -ExclusionPath "C:\Users\MichaelBrown\.rustup"
    Add-MpPreference -ExclusionPath "C:\Users\MichaelBrown\AppData\Roaming\npm"
    
    # Add process exclusions
    Write-Host "Adding process exclusions..." -ForegroundColor Yellow
    Add-MpPreference -ExclusionProcess "cargo.exe"
    Add-MpPreference -ExclusionProcess "rustc.exe"
    Add-MpPreference -ExclusionProcess "node.exe"
    Add-MpPreference -ExclusionProcess "pnpm.exe"
    Add-MpPreference -ExclusionProcess "npm.exe"
    Add-MpPreference -ExclusionProcess "vite.exe"
    
    # Add common build output file types
    Write-Host "Adding file extension exclusions..." -ForegroundColor Yellow
    Add-MpPreference -ExclusionExtension ".exe"
    Add-MpPreference -ExclusionExtension ".dll"
    Add-MpPreference -ExclusionExtension ".pdb"
    
    Write-Host "Windows Defender exclusions added successfully!" -ForegroundColor Green
    Write-Host "Your development environment should now build faster without Defender interference." -ForegroundColor Green
    
} catch {
    Write-Host "Error adding exclusions: $($_.Exception.Message)" -ForegroundColor Red
}

pause