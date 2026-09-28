# Tareas del proyecto. `make help` las lista.
.PHONY: help run build web server test test-rust test-web e2e synth types docker clean

help:
	@echo "make run      compila lo que falte y abre la aplicación (igual que ./pss)"
	@echo "make build    compila la interfaz y el servidor"
	@echo "make test     pruebas de Rust y de la interfaz (sin navegador)"
	@echo "make e2e      pruebas de extremo a extremo con Playwright"
	@echo "make synth    regenera traces/synthetic desde tools/synth"
	@echo "make types    regenera web/src/trace/types.ts desde el esquema"
	@echo "make docker   construye la imagen local"

run:
	./pss

build: web server

web:
	cd web && npm ci --no-audit --no-fund && npm run build

server:
	cargo build --release -p pss-server -p pss-tracer

test: test-rust test-web

test-rust:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo test

test-web:
	cd web && npm run gen:types:check && npm run typecheck && npm test

e2e: server
	cd web && npm run build && npm run e2e

synth:
	cd web && npm run gen:synth

types:
	cd web && npm run gen:types

docker:
	docker build -t pss-visualizer .

clean:
	cargo clean
	rm -rf web/dist web/test-results web/playwright-report
