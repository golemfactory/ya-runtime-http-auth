.PHONY: clean arm64
VERSION := $(shell grep -m 1 "version" Cargo.toml | sed -e "s/version = //" | tr -d '"')
PKG_NAME := "ya-runtime-basic-auth-arm64-v$(VERSION)"

target/aarch64-unknown-linux-musl/release/ya-runtime-basic-auth:
	docker run -it --rm \
		--workdir /git-repo \
		--volume $(shell pwd):/git-repo \
		--volume /etc/passwd:/etc/passwd \
		--env HOST_USER=`id -u`:`id -g` \
		golemfactory/build-aarch64:0.1.1 \
		bash -c 'cargo build --release && chown -R $$HOST_USER ./target'

arm64: target/aarch64-unknown-linux-musl/release/ya-runtime-basic-auth
	mkdir -p "$(PKG_NAME)/ya-runtime-basic-auth"
	cp target/aarch64-unknown-linux-musl/release/ya-runtime-basic-auth "$(PKG_NAME)/ya-runtime-basic-auth"
	sed -e 's/<VERSION>/$(VERSION)/' runtime-descriptor-template.json > "$(PKG_NAME)/ya-runtime-basic-auth.json"
	tar -cvzf "$(PKG_NAME).tar.gz" $(PKG_NAME)
	rm -rf $(PKG_NAME)

clean:
	rm -rf target
	rm -rf $(PKG_NAME)
	rm -f "$(PKG_NAME).tar.gz"
