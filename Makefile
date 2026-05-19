.PHONY: dev build test reset-demo demo logs clean

## Start all services with Docker Compose
dev:
	docker compose up

## Build release binaries + admin bundle
build:
	cargo build --release --manifest-path apps/backend/Cargo.toml
	cd apps/admin && npm run build

## Run backend tests
test:
	cargo test --manifest-path apps/backend/Cargo.toml

## Reset and seed demo data (requires built binaries)
reset-demo:
	./scripts/reset-demo.sh

## Full demo setup: build + reset + start
demo: build reset-demo
	docker compose up -d
	@echo ""
	@echo "NexusMind demo running:"
	@echo "  Backend: http://localhost:8080/v1/health"
	@echo "  Admin:   http://localhost:3000"
	@echo "  Key:     nm_demo_acme_admin"

## Follow Docker logs
logs:
	docker compose logs -f

## Stop and remove containers
clean:
	docker compose down
