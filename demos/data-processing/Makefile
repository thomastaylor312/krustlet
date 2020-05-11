COLOR ?= always # Valid COLOR options: {always, auto, never}
TARGET ?= target/wasm32-unknown-unknown
ifndef DEBUG 
RELEASE = "--release"
BINARY_PATH = $(TARGET)/release
else
RELEASE = ""
BINARY_PATH = $(TARGET)/debug
endif
CARGO = cargo --color $(COLOR)
KEYDIR ?= .keys
VERSION ?= 0.1.0
SIGNED ?= _signed

.PHONY: all bench build check clean doc test update keys

all: build-ingest build-result build-processor

build-ingest:
	CRATE_LOCATION=crates/ingest NAME=ingest $(MAKE) build sign

build-result:
	CRATE_LOCATION=crates/result NAME=result $(MAKE) build sign

build-processor:
	CRATE_LOCATION=. NAME=processor TARGET=target/wasm32-wasi $(MAKE) build

build: keys
	@cd $(CRATE_LOCATION) && $(CARGO) build $(RELEASE)

sign:
	wascap sign $(BINARY_PATH)/$(NAME).wasm $(BINARY_PATH)/$(NAME)$(SIGNED).wasm -i $(KEYDIR)/account.nk -u $(KEYDIR)/module.nk -s -f -l -n $(NAME)

push-ingest:
	CRATE_LOCATION=crates/ingest NAME=ingest $(MAKE) push

push-result:
	CRATE_LOCATION=crates/result NAME=result $(MAKE) push

push-processor:
	CRATE_LOCATION=. NAME=processor TARGET=target/wasm32-wasi $(MAKE) push

push:
	wasm-to-oci push $(BINARY_PATH)/$(NAME)$(SIGNED).wasm webassembly.azurecr.io/$(NAME):v$(VERSION)

keys: $(KEYDIR)/account.nk $(KEYDIR)/module.nk

$(KEYDIR)/account.nk:
	@mkdir -p $(KEYDIR)
	nk gen account > $(KEYDIR)/account.txt
	awk '/Seed/{ print $$2 }' $(KEYDIR)/account.txt > $(KEYDIR)/account.nk

$(KEYDIR)/module.nk:
	@mkdir -p $(KEYDIR)
	nk gen module > $(KEYDIR)/module.txt
	awk '/Seed/{ print $$2 }' $(KEYDIR)/module.txt > $(KEYDIR)/module.nk
