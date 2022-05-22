SET VERSION=0.2.2

RMDIR /Q /S release || exit /b
MKDIR release\mprocs-%VERSION%-win64 || exit /b

:: Windows 64

cargo build --release || exit /b

COPY target\release\mprocs.exe release\mprocs-%VERSION%-win64\mprocs.exe || exit /b

:: upx --brute release\mprocs-%VERSION%-win64\mprocs.exe || exit /b

tar.exe -a -c -f release\mprocs-%VERSION%-win64.zip -C release\mprocs-%VERSION%-win64 mprocs.exe
