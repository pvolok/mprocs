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

ifeq ($(OSNAME),windows)
	BIN_EXT=.exe
else
	BIN_EXT=
endif

DIST=dist
OUT=$(DIST)/$(PLAT)
BIN_NAME=mprocs$(BIN_EXT)
ZIP_OUT=$(DIST)/mprocs-$(PLAT).zip

ROOT=$(shell pwd)

.PHONY: build
build:
	rm -rf $(OUT)
	rm -rf $(ZIP_OUT)
	mkdir -p $(OUT)
	
	opam install . --deps-only
	opam exec dune build bin/mprocs.exe
	strip -o $(OUT)/$(BIN_NAME) _build/default/bin/mprocs.exe
	chmod +w $(OUT)/$(BIN_NAME)
	upx --best --overlay=strip $(OUT)/$(BIN_NAME)
	zip $(ZIP_OUT) $(OUT)/*

build-alpine:
	docker build -t mprocs-build-alpine build/alpine/
	docker run --name mprocs-build-alpine2 --rm \
		-v $(ROOT)/Makefile:/app/Makefile \
		-v $(ROOT)/bin:/app/bin \
		-v $(ROOT)/dist:/app/dist \
		-v $(ROOT)/dune:/app/dune \
		-v $(ROOT)/dune-project:/app/dune-project \
		-v $(ROOT)/mprocs.opam:/app/mprocs.opam \
		-v $(ROOT)/pty:/app/pty \
		-v $(ROOT)/src:/app/src \
		-v $(ROOT)/tui:/app/tui \
		-v $(ROOT)/vterm:/app/vterm \
		mprocs-build-alpine make build
