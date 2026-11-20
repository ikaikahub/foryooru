# runs the app on a connected phone with live reload, using the SDK installed by Android Studio.
# extra arguments go to tauri, e.g. --host 127.0.0.1 after adb reverse when Wi-Fi doesn't reach the phone
$ErrorActionPreference = 'Stop'

$sdkPath = "$env:LOCALAPPDATA\Android\Sdk"
# keep in sync with the NDK version in .github/workflows/build.yml
$ndkPath = "$sdkPath\ndk\29.0.13846066"

if (-not (Test-Path $ndkPath)) {
  throw "NDK not found at $ndkPath, install it from the SDK Manager in Android Studio"
}

$env:ANDROID_HOME = $sdkPath
$env:NDK_HOME = $ndkPath

Push-Location (Join-Path $PSScriptRoot '..')
try {
  pnpm tauri android dev @args
  exit $LASTEXITCODE
} finally {
  Pop-Location
}
