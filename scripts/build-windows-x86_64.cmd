SET VERSION=0.8.3

MKDIR release\mprocs-%VERSION%-windows-x86_64 || exit /b

:: Windows x64

cargo build --release || exit /b

COPY target\release\mprocs.exe release\mprocs-%VERSION%-windows-x86_64\mprocs.exe || exit /b

tar.exe -a -c -f release\mprocs-%VERSION%-windows-x86_64.zip -C release\mprocs-%VERSION%-windows-x86_64 mprocs.exe
