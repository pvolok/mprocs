ifeq ($(OS),Windows_NT)
    OSNAME = windows
    ifeq ($(PROCESSOR_ARCHITEW6432),AMD64)
        PLAT = $(OSNAME)-amd64
    else
        ifeq ($(PROCESSOR_ARCHITECTURE),AMD64)
            PLAT = $(OSNAME)-amd64
        endif
        ifeq ($(PROCESSOR_ARCHITECTURE),x86)
            PLAT = $(OSNAME)-x86
        endif
    endif
else
    UNAME_S := $(shell uname -s)
    ifeq ($(UNAME_S),Linux)
        OSNAME=linux
    endif
    ifeq ($(UNAME_S),Darwin)
	OSNAME=macos
    endif
    UNAME_M := $(shell uname -m)
    ifeq ($(UNAME_M),x86_64)
	PLAT = $(OSNAME)-amd64
    endif
    ifneq ($(filter %86,$(UNAME_M)),)
        PLAT = $(OSNAME)-x86
    endif
    ifneq ($(filter arm%,$(UNAME_M)),)
        PLAT = $(OSNAME)-arm
    endif
endif

build:
	rm -rf dist
	dune build --profile=release bin/mprocs.exe
	mkdir -p dist
	strip -o dist/mprocs _build/default/bin/mprocs.exe
	chmod +w dist/mprocs
	upx --best --overlay=strip dist/mprocs
	zip dist/mprocs-$(PLAT) dist/mprocs
