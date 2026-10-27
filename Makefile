.DEFAULT_GOAL := all
CARGO ?= cargo
CC ?= cc
PYTHON ?= python3
PREFIX ?= /usr
MULTIARCH := $(shell dpkg-architecture -qDEB_HOST_MULTIARCH)
LIBDIR ?= $(PREFIX)/lib/$(MULTIARCH)
DESTDIR ?=
VERSION := 0.1.0-alpha1
BACKENDS := g711 pcm gsm opus speex spandsp bcg729 ilbc dialogic ffmpeg
LIBRARIES := $(addprefix librptadv_codec_,$(BACKENDS))
SONAMES := $(addsuffix .so.1,$(LIBRARIES))
LIBVERSION := 1.0.0
# The complete gate excludes nothing; focused targets select source parts only.
COVERAGE_EXCLUDE ?= a^
RUST_SOURCES := $(shell find rust -type f \( -name '*.rs' -o -name '*.c' -o -name 'Cargo.toml' \))
TEST_C_SOURCES := $(wildcard tests/*.c)
EXTERNAL_SOURCES := $(shell find packaging/external -type f ! -name '*.pyc')
export RPTADV_CODEC_BACKEND_LIBDIR ?= $(CURDIR)/build/backends/lib
export LD_LIBRARY_PATH := $(RPTADV_CODEC_BACKEND_LIBDIR)$(if $(LD_LIBRARY_PATH),:$(LD_LIBRARY_PATH))

.PHONY: all external backend-check sources-check test abi-check lint static-analysis docs quality install install-check coverage coverage-foundations package-check debian-package-check platform-verify clean

all: $(addprefix build/,$(SONAMES))

build/librptadv_codec_%.so.1: Cargo.toml Cargo.lock $(RUST_SOURCES) rust/ffmpeg/src/glue.c | backend-check
	$(CARGO) rustc -p rptadv-codec-$* --release --locked -- -C link-arg=-Wl,-soname,librptadv_codec_$*.so.1
	mkdir -p build
	cp target/release/librptadv_codec_$*.so build/librptadv_codec_$*.so.$(LIBVERSION)
	ln -sfn librptadv_codec_$*.so.$(LIBVERSION) $@
	ln -sfn librptadv_codec_$*.so.1 build/librptadv_codec_$*.so
	readelf -d $@ | grep -F 'librptadv_codec_$*.so.1'
	! readelf -d $@ | grep -E 'libasterisk|libportaudio|libasound|res_usbradio'

backend-check:
	test -f "$(RPTADV_CODEC_BACKEND_LIBDIR)/librptadv_bcg729_gpl2.so.0"
	test -f "$(RPTADV_CODEC_BACKEND_LIBDIR)/librptadv_ilbc2.so.2"
	test -f "$(RPTADV_CODEC_BACKEND_LIBDIR)/librptadv_dialogic.so.1"

external: build/backends/.built

build/backends/.built: dependencies/codecs.json $(EXTERNAL_SOURCES)
	$(PYTHON) packaging/external/build.py bcg729 --prefix build/backends
	$(PYTHON) packaging/external/build.py ilbc --prefix build/backends
	$(PYTHON) packaging/external/build.py dialogic --prefix build/backends
	touch $@

sources-check:
	$(PYTHON) -m unittest discover -s tests -p test_sources.py

test: all sources-check
	$(CARGO) test --workspace --locked
	RPTADV_CODEC_DIR="$(CURDIR)/build" RPTADV_CODEC_PCM_LIBRARY="$(CURDIR)/build/librptadv_codec_pcm.so.1" RPTADV_CODEC_G711_LIBRARY="$(CURDIR)/build/librptadv_codec_g711.so.1" $(PYTHON) -m unittest discover -s tests
	$(MAKE) abi-check

abi-check: all
	$(CC) -std=c11 -Wall -Wextra -Werror -Iinclude tests/abi_header_smoke.c -Lbuild -lrptadv_codec_g711 -Wl,-rpath,'$$ORIGIN' -o build/abi-header-smoke
	./build/abi-header-smoke

lint:
	$(CARGO) fmt --all -- --check
	ruff format --check tests packaging
	ruff check tests packaging
	clang-format --dry-run --Werror include/rptadv_codec.h $(TEST_C_SOURCES) rust/ffmpeg/src/glue.c packaging/external/dialogic/dialogic.c

static-analysis:
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	cppcheck --enable=warning,style,performance,portability --error-exitcode=1 --force --std=c11 -Iinclude $(TEST_C_SOURCES) rust/ffmpeg/src/glue.c packaging/external/dialogic/dialogic.c

docs:
	mkdir -p build
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps --locked
	doxygen Doxyfile
	test ! -s build/doxygen-warnings.log

quality: lint static-analysis docs

install: all
	install -d $(DESTDIR)$(LIBDIR)/pkgconfig $(DESTDIR)$(PREFIX)/include/rptadv-codec
	set -eu; for library in $(LIBRARIES); do \
	  install -m 0755 build/$$library.so.$(LIBVERSION) $(DESTDIR)$(LIBDIR)/; \
	  ln -sfn $$library.so.$(LIBVERSION) $(DESTDIR)$(LIBDIR)/$$library.so.1; \
	  ln -sfn $$library.so.1 $(DESTDIR)$(LIBDIR)/$$library.so; \
	done
	cp -a "$(RPTADV_CODEC_BACKEND_LIBDIR)"/librptadv_bcg729_gpl2.so* "$(RPTADV_CODEC_BACKEND_LIBDIR)"/librptadv_ilbc2.so* "$(RPTADV_CODEC_BACKEND_LIBDIR)"/librptadv_dialogic.so* $(DESTDIR)$(LIBDIR)/
	for backend in bcg729 ilbc dialogic; do \
	  install -d $(DESTDIR)$(PREFIX)/share/doc/rptadv-$$backend; \
	  install -m 0644 "$(RPTADV_CODEC_BACKEND_LIBDIR)/../share/doc/rptadv-$$backend/copyright.upstream" $(DESTDIR)$(PREFIX)/share/doc/rptadv-$$backend/; \
	done
	install -m 0644 include/rptadv_codec.h $(DESTDIR)$(PREFIX)/include/rptadv-codec/
	sed -e 's|@PREFIX@|$(PREFIX)|' -e 's|@LIBDIR@|$(LIBDIR)|' rptadv_codec.pc.in > $(DESTDIR)$(LIBDIR)/pkgconfig/rptadv_codec.pc

install-check: all
	set -eu; stage=$$(mktemp -d "$(CURDIR)/build/install.XXXXXX"); \
	trap 'rm -rf "$$stage"' EXIT; \
	$(MAKE) DESTDIR="$$stage" install; \
	for library in $(LIBRARIES); do test -f "$$stage$(LIBDIR)/$$library.so.1"; done; \
	test -f "$$stage$(PREFIX)/include/rptadv-codec/rptadv_codec.h"; \
	test -z "$$(find "$$stage" -name '*.a')"

coverage:
	mkdir -p build/coverage
	+@set -e; export RUSTUP_TOOLCHAIN=nightly-2025-02-20; \
		export CC=clang CXX=clang++; unset LLVM_COV LLVM_PROFDATA; \
		export CARGO_LLVM_COV_TARGET_DIR="$(CURDIR)/target/codec-coverage"; \
		export CARGO_TARGET_DIR="$$CARGO_LLVM_COV_TARGET_DIR"; \
		eval "$$($(CARGO) llvm-cov show-env --branch --include-ffi --export-prefix)"; \
		$(CARGO) llvm-cov clean --workspace; \
		$(CARGO) build --locked --workspace --lib; \
		$(CARGO) test --locked --workspace --all-targets; \
		RPTADV_CODEC_DIR="$$CARGO_TARGET_DIR/debug" \
		RPTADV_CODEC_G711_LIBRARY="$$CARGO_TARGET_DIR/debug/librptadv_codec_g711.so" \
		RPTADV_CODEC_PCM_LIBRARY="$$CARGO_TARGET_DIR/debug/librptadv_codec_pcm.so" \
		$(PYTHON) -m unittest discover -s tests; \
		$(CARGO) llvm-cov report --include-ffi --ignore-filename-regex "$(COVERAGE_EXCLUDE)" --json --output-path build/coverage/rust.json
	$(PYTHON) tests/check_coverage.py build/coverage/rust.json

coverage-foundations:
	$(MAKE) coverage COVERAGE_EXCLUDE='rust/(bcg729|gsm|ffmpeg|ilbc|opus|spandsp|speex)/'

package-check: all
	set -eu; stage=$$(mktemp -d "$(CURDIR)/build/package.XXXXXX"); \
	trap 'rm -rf "$$stage"' EXIT; \
	tar --exclude='./.git' --exclude='./.work' --exclude='./build' --exclude='./target' -cf - . | tar -xf - -C "$$stage"; \
	chmod 0755 "$$stage/debian/rules"; \
	cd "$$stage"; dpkg-buildpackage -us -uc -b

debian-package-check: package-check

platform-verify: test install-check package-check

clean:
	$(CARGO) clean
	rm -rf build
