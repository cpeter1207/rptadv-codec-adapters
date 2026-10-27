.DEFAULT_GOAL := all
CARGO ?= cargo
CC ?= cc
PYTHON ?= python3
PREFIX ?= /usr
MULTIARCH := $(shell dpkg-architecture -qDEB_HOST_MULTIARCH)
LIBDIR ?= $(PREFIX)/lib/$(MULTIARCH)
DESTDIR ?=
VERSION := 0.1.0-alpha1
BACKENDS := g711 pcm gsm opus speex spandsp
LIBRARIES := $(addprefix librptadv_codec_,$(BACKENDS))
SONAMES := $(addsuffix .so.1,$(LIBRARIES))
LIBVERSION := 1.0.0
RUST_SOURCES := $(shell find rust -name '*.rs' -type f)

.PHONY: all sources-check test abi-check lint static-analysis docs quality install install-check coverage package-check debian-package-check platform-verify clean

all: $(addprefix build/,$(SONAMES))

build/librptadv_codec_%.so.1: Cargo.toml Cargo.lock $(RUST_SOURCES)
	$(CARGO) rustc -p rptadv-codec-$* --release --locked -- -C link-arg=-Wl,-soname,librptadv_codec_$*.so.1
	mkdir -p build
	cp target/release/librptadv_codec_$*.so build/librptadv_codec_$*.so.$(LIBVERSION)
	ln -sfn librptadv_codec_$*.so.$(LIBVERSION) $@
	ln -sfn librptadv_codec_$*.so.1 build/librptadv_codec_$*.so
	readelf -d $@ | grep -F 'librptadv_codec_$*.so.1'
	! readelf -d $@ | grep -E 'libasterisk|libportaudio|libasound|res_usbradio'

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
	ruff format --check tests
	ruff check tests
	clang-format --dry-run --Werror include/rptadv_codec.h tests/abi_header_smoke.c

static-analysis:
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	cppcheck --enable=warning,style,performance,portability --error-exitcode=1 --force --std=c11 -Iinclude tests/abi_header_smoke.c

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
	$(CARGO) +nightly-2025-02-20 llvm-cov --workspace --all-targets --branch --json --output-path build/coverage/rust.json
	$(PYTHON) tests/check_coverage.py build/coverage/rust.json

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
