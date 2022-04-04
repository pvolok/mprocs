SET VERSION=0.1.0

RMDIR \Q \S release
MD release

:: Windows 64

cargo build --release

COPY target\release\mprocs.exe release\mprocs-%VERSION%-win64.exe

upx --brute release\mprocs-%VERSION%-win64.exe
