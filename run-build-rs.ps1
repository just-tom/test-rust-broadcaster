$env:Path = [System.Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [System.Environment]::GetEnvironmentVariable('Path', 'User')
$env:PKG_CONFIG_PATH = 'C:\vcpkg\installed\x64-windows-static\lib\pkgconfig'
$env:LIBCLANG_PATH = 'C:\vcpkg\downloads\tools\clang\clang-15.0.6\bin'
$env:X264_LIB_DIR = 'C:\vcpkg\installed\x64-windows-static\lib'
$env:X264_INCLUDE_DIR = 'C:\vcpkg\installed\x64-windows-static\include'
$env:FDK_AAC_LIB_DIR = 'C:\vcpkg\installed\x64-windows-static\lib'
$env:FDK_AAC_INCLUDE_DIR = 'C:\vcpkg\installed\x64-windows-static\include'
$env:CARGO_LOG = 'debug'
Set-Location 'C:\Users\Admin\Desktop\test-rust-broadcaster'
cargo clean -p broadcaster
cargo check -p broadcaster -vv 2>&1 | Select-Object -First 200
